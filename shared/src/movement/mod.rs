use crate::collision::collider::{snake_friction, SnakeFrictionEvent};
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use bevy_replicon::prelude::{Diffable as RepliconDiffable, PatchHistory};
use lightyear::prelude::input::bei::Fire;

use crate::config::GameConfig;
use crate::network::protocol::components::snake::Direction;
use crate::network::protocol::prelude::*;
use crate::utils::query::{Simulated, SimulationAuthority};
use lightyear::prelude::LocalTimeline;

pub struct MovementPlugin;

pub type TailPointsDiffLog = PatchHistory<TailPoints>;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum SimulationSet {
    // move snakes
    Movement,
}

pub const MIN_SPEED: f32 = 0.85;
pub const MAX_SPEED: f32 = 4.0;
pub const BASE_ACCELERATION: f32 = -0.01;
pub const ACCELERATION_RATIO: f32 = 2.0;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // events
        app.add_message::<SnakeFrictionEvent>();

        // sets
        app.configure_sets(FixedUpdate, SimulationSet::Movement);

        // 1. turn heads if we received inputs -> done automatically during replication
        // 2. update acceleration (are there close snakes?)
        // 3. update the front of the tail: possibly add a new inflection point if necessary (if direction changed)
        // 4. update heads: integrate acceleration into velocity, integrate velocity into position
        // 5. update the back of the tails: shorten tail
        app.add_observer(turn_head_from_input);
        app.add_systems(
            FixedUpdate,
            (update_acceleration.after(snake_friction), update_tails)
                .chain()
                .in_set(SimulationSet::Movement),
        );
    }
}

// 1. turn heads according to input
pub fn turn_head_from_input(
    trigger: On<Fire<MoveSnake>>,
    mut query: Query<
        (
            &mut SnakeHead,
            &mut TailPoints,
            Option<&mut PatchHistory<TailPoints>>,
        ),
        Simulated,
    >,
) {
    let Some(direction) = direction_from_input(trigger.value) else {
        return;
    };
    let Ok((mut head, mut tail, log)) = query.get_mut(trigger.context) else {
        return;
    };
    let _ = turn_tail_with_log(&mut head, &mut tail, log, direction);
}

pub fn direction_from_input(input: Vec2) -> Option<Direction> {
    if input == Vec2::ZERO {
        return None;
    }
    if input.y.abs() >= input.x.abs() {
        if input.y > 0.0 {
            Some(Direction::Up)
        } else {
            Some(Direction::Down)
        }
    } else if input.x > 0.0 {
        Some(Direction::Right)
    } else {
        Some(Direction::Left)
    }
}

pub fn turn_tail(head: &mut SnakeHead, tail: &mut TailPoints, requested: Direction) {
    let _ = turn_tail_with_log(head, tail, None, requested);
}

pub fn turn_tail_with_log<'a>(
    head: &mut SnakeHead,
    tail: &mut TailPoints,
    log: Option<Mut<'a, PatchHistory<TailPoints>>>,
    requested: Direction,
) -> Option<Mut<'a, PatchHistory<TailPoints>>> {
    let current = head.direction;
    if is_perpendicular_turn(current, requested) {
        let log = apply_tail_op(
            tail,
            log,
            TailPointsOp::PushTurn(TailTurn::new(head.position, current.opposite())),
        );
        head.direction = requested;
        return log;
    }
    log
}

pub fn is_perpendicular_turn(current: Direction, requested: Direction) -> bool {
    !matches!(
        (current, requested),
        (
            Direction::Up | Direction::Down,
            Direction::Up | Direction::Down
        ) | (
            Direction::Left | Direction::Right,
            Direction::Left | Direction::Right
        )
    )
}

// 2. update acceleration (are there close snakes?)
// - we start accelerating when we are close to another snake
// - otherwise we keep decelerating until we reach minimum speed
// - i'd like to add some easing; i.e have the change in acceleration not take place instantly
pub fn update_acceleration(
    mut events: MessageReader<SnakeFrictionEvent>,
    config: Res<GameConfig>,
    mut snakes: Query<(Entity, &mut Acceleration, &mut FoodBoost), Simulated>,
) {
    let movement = &config.movement;
    let mut proximity_boosts = EntityHashMap::default();
    for event in events.read() {
        let acceleration = boost_acceleration(
            movement.base_acceleration,
            movement.boost_acceleration_ratio,
            movement.boost_distance,
            event.distance,
        );
        proximity_boosts
            .entry(event.main)
            .and_modify(|current: &mut f32| *current = current.max(acceleration))
            .or_insert(acceleration);
    }

    for (entity, mut acceleration, mut food_boost) in snakes.iter_mut() {
        acceleration.set_if_neq(Acceleration(combined_acceleration(
            movement.base_acceleration,
            proximity_boosts.get(&entity).copied(),
            food_boost.0,
        )));
        food_boost.0 = decayed_food_boost(food_boost.0, movement.food_boost_decay);
    }
}

pub fn combined_acceleration(
    base_acceleration: f32,
    proximity_acceleration: Option<f32>,
    food_boost: f32,
) -> f32 {
    proximity_acceleration.unwrap_or(base_acceleration) + food_boost
}

pub fn decayed_food_boost(boost: f32, decay: f32) -> f32 {
    let decayed = boost * decay.clamp(0.0, 1.0);
    if decayed.abs() < 0.001 {
        0.0
    } else {
        decayed
    }
}

pub fn boost_acceleration(
    base_acceleration: f32,
    boost_acceleration_ratio: f32,
    boost_distance: f32,
    distance: f32,
) -> f32 {
    if boost_distance <= 0.0 {
        return 0.0;
    }
    base_acceleration.abs() * boost_acceleration_ratio * (boost_distance - distance).max(0.0)
        / boost_distance
}

// 3. update front of the tail: possibly add a new inflection point if necessary
// 4. update acceleration and speed
// 5. update the back of the tails: shorten tail
pub fn update_tails(
    config: Res<GameConfig>,
    timeline: Option<Res<LocalTimeline>>,
    mut query: Query<
        (
            &mut SnakeHead,
            &mut TailPoints,
            Option<&mut PatchHistory<TailPoints>>,
            &mut TailLength,
            &mut Speed,
            &Acceleration,
            Has<SimulationAuthority>,
            Option<&mut TailPathHistory>,
        ),
        Simulated,
    >,
) {
    let movement = &config.movement;
    let lag_compensation_enabled = config.network.lag_compensation.enabled;
    let retained_extra_length = if lag_compensation_enabled {
        lag_compensation_extra_length(&config)
    } else {
        0.0
    };
    let max_history_samples = usize::from(config.network.lag_compensation.max_delay_ticks) + 3;
    for (
        mut head,
        mut tail,
        log,
        mut length,
        mut speed,
        acceleration,
        has_simulation_authority,
        mut history,
    ) in query.iter_mut()
    {
        // 4. update acceleration and speed
        // update velocity
        // do not update speed if we are at min speed and acceleration is negative
        // do not update speed if we are at max speed and acceleration is positive
        if !((acceleration.0 < 0.0 && speed.0 == movement.min_speed)
            || (acceleration.0 > 0.0 && speed.0 == movement.max_speed))
        {
            speed.0 += acceleration.0;
            speed.0 = speed.0.max(movement.min_speed).min(movement.max_speed);
        }

        // update position
        let next_position = head.position + head.direction.delta() * speed.0;
        let moved_distance = head.position.distance(next_position);
        head.position = next_position;
        if lag_compensation_enabled {
            if let Some(history) = history.as_deref_mut() {
                history.advance_head(moved_distance);
            }
        }
        length.current_size += speed.0;

        // 5. update the back of the tails: shorten tail
        // NOTE: it's ok to activate change detection here because we already updated the snake anyway
        let retained_extra_length = if has_simulation_authority {
            retained_extra_length
        } else {
            0.0
        };
        let _ = shorten_tail_with_retention_with_log(
            head.as_ref(),
            &mut tail,
            log,
            length.as_mut(),
            retained_extra_length,
        );
        if lag_compensation_enabled {
            if let (Some(timeline), Some(history)) = (timeline.as_ref(), history.as_deref_mut()) {
                history.record_sample(timeline.tick(), length.current_size, max_history_samples);
            }
        }
    }
}

fn apply_tail_op<'a>(
    tail: &mut TailPoints,
    mut log: Option<Mut<'a, PatchHistory<TailPoints>>>,
    op: TailPointsOp,
) -> Option<Mut<'a, PatchHistory<TailPoints>>> {
    RepliconDiffable::apply_patch(tail, &op)
        .expect("tail diff patches should be valid for live TailPoints");
    if let Some(log) = log.as_mut() {
        log.record(op);
    }
    log
}

/// Shorten the tail to match the target size
pub fn shorten_tail(head: &SnakeHead, tail: &mut TailPoints, tail_length: &mut TailLength) {
    // if we still need to grow the tail, do nothing
    if tail_length.target_size >= tail_length.current_size {
        return;
    }

    let _ = tail.prune_to_length(head, tail_length.target_size);
    tail_length.current_size = tail_length.target_size;
}

#[cfg(test)]
fn shorten_tail_with_retention(
    head: &SnakeHead,
    tail: &mut TailPoints,
    tail_length: &mut TailLength,
    retained_extra_length: f32,
) {
    let _ =
        shorten_tail_with_retention_with_log(head, tail, None, tail_length, retained_extra_length);
}

fn shorten_tail_with_retention_with_log<'a>(
    head: &SnakeHead,
    tail: &mut TailPoints,
    log: Option<Mut<'a, PatchHistory<TailPoints>>>,
    tail_length: &mut TailLength,
    retained_extra_length: f32,
) -> Option<Mut<'a, PatchHistory<TailPoints>>> {
    let mut log = log;
    if tail_length.target_size < tail_length.current_size {
        tail_length.current_size = tail_length.target_size;
    }

    let retained_length = tail_length.current_size + retained_extra_length.max(0.0);
    let removed = tail.prune_to_length(head, retained_length);
    if removed > 0 {
        let op = TailPointsOp::RemoveTailTurns(removed.min(u16::MAX as usize) as u16);
        if let Some(log) = log.as_mut() {
            log.record(op);
        }
    }
    log
}

pub fn lag_compensation_extra_length(config: &GameConfig) -> f32 {
    if !config.network.lag_compensation.enabled {
        return 0.0;
    }
    config.movement.max_speed.max(0.0) * f32::from(config.network.lag_compensation.max_delay_ticks)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::network::bundle::snake::SnakeBundle;
    use crate::utils::query::SimulationAuthority;
    use lightyear::prelude::{Interpolated, Predicted, Replicated};

    fn create_snake(app: &mut App) -> Entity {
        let (head, length, tail) = TailPoints::from_legacy_polyline(VecDeque::from(vec![
            (Vec2::new(50.0, 100.0), Direction::Right),
            (Vec2::new(0.0, 100.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]));
        app.world_mut()
            .spawn((
                head,
                tail,
                length,
                Speed(0.5),
                Acceleration(0.0),
                FoodBoost::default(),
            ))
            .id()
    }

    fn run_fixed_update(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
    }

    fn head_position(app: &App, snake: Entity) -> Vec2 {
        app.world()
            .entity(snake)
            .get::<SnakeHead>()
            .unwrap()
            .position
    }

    fn visible_tail(app: &App, snake: Entity) -> TailPolyline {
        let entity = app.world().entity(snake);
        let head = entity.get::<SnakeHead>().unwrap();
        let tail = entity.get::<TailPoints>().unwrap();
        let length = entity.get::<TailLength>().unwrap();
        tail.polyline(head, length.current_size)
    }

    #[test]
    fn direction_from_input_prefers_vertical_on_tie() {
        assert_eq!(direction_from_input(Vec2::Y), Some(Direction::Up));
        assert_eq!(direction_from_input(-Vec2::Y), Some(Direction::Down));
        assert_eq!(direction_from_input(Vec2::X), Some(Direction::Right));
        assert_eq!(direction_from_input(-Vec2::X), Some(Direction::Left));
        assert_eq!(
            direction_from_input(Vec2::new(1.0, 1.0)),
            Some(Direction::Up)
        );
        assert_eq!(direction_from_input(Vec2::ZERO), None);
    }

    #[test]
    fn turn_tail_rejects_same_axis_turns() {
        let mut head = SnakeHead {
            position: Vec2::ZERO,
            direction: Direction::Up,
        };
        let mut tail = TailPoints::empty();

        turn_tail(&mut head, &mut tail, Direction::Down);
        assert_eq!(head.direction, Direction::Up);
        assert!(tail.turns.is_empty());

        turn_tail(&mut head, &mut tail, Direction::Right);
        assert_eq!(head.direction, Direction::Right);
        assert_eq!(
            tail.turns,
            VecDeque::from([TailTurn::new(Vec2::ZERO, Direction::Down)])
        );

        turn_tail(&mut head, &mut tail, Direction::Left);
        assert_eq!(head.direction, Direction::Right);
        assert_eq!(tail.turns.len(), 1);
    }

    #[test]
    fn perpendicular_turn_detection_rejects_noop_and_reverse() {
        assert!(!is_perpendicular_turn(Direction::Up, Direction::Up));
        assert!(!is_perpendicular_turn(Direction::Up, Direction::Down));
        assert!(is_perpendicular_turn(Direction::Up, Direction::Left));
        assert!(is_perpendicular_turn(Direction::Left, Direction::Down));
        assert!(!is_perpendicular_turn(Direction::Left, Direction::Right));
    }

    #[test]
    fn boost_acceleration_scales_with_distance() {
        assert!((boost_acceleration(-0.01, 2.0, 20.0, 20.0) - 0.0).abs() < f32::EPSILON);
        assert!((boost_acceleration(-0.01, 2.0, 20.0, 0.0) - 0.02).abs() < f32::EPSILON);
        assert!((boost_acceleration(-0.01, 2.0, 20.0, 10.0) - 0.01).abs() < f32::EPSILON);
    }

    #[test]
    fn food_boost_decays_smoothly() {
        assert!((decayed_food_boost(0.03, 0.94) - 0.0282).abs() < f32::EPSILON);
        assert_eq!(decayed_food_boost(0.0001, 0.94), 0.0);
        assert_eq!(decayed_food_boost(0.03, -1.0), 0.0);
    }

    #[test]
    fn food_boost_layers_over_base_or_proximity_acceleration() {
        assert!((combined_acceleration(-0.01, None, 0.03) - 0.02).abs() < f32::EPSILON);
        assert!((combined_acceleration(-0.01, Some(0.02), 0.03) - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn replicated_interpolated_authoritative_snake_moves() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.add_plugins(MovementPlugin);

        let snake = app
            .world_mut()
            .spawn((
                SnakeBundle::default(),
                Replicated,
                Interpolated,
                SimulationAuthority,
            ))
            .id();
        let before = head_position(&app, snake);

        run_fixed_update(&mut app);

        let after = head_position(&app, snake);
        assert_ne!(after, before);
    }

    #[test]
    fn authoritative_snake_tail_records_diff_mutations() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.add_plugins(MovementPlugin);

        let snake = app
            .world_mut()
            .spawn((
                SnakeBundle::default(),
                PatchHistory::<TailPoints>::default(),
                Replicated,
                Interpolated,
                SimulationAuthority,
            ))
            .id();

        run_fixed_update(&mut app);

        assert_eq!(
            app.world()
                .entity(snake)
                .get::<PatchHistory<TailPoints>>()
                .unwrap()
                .current_cursor(),
            None
        );
    }

    #[test]
    fn predicted_snake_tail_records_local_diff_mutations() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.add_plugins(MovementPlugin);

        let snake = app
            .world_mut()
            .spawn((
                SnakeBundle::default(),
                PatchHistory::<TailPoints>::default(),
                Predicted,
            ))
            .id();
        let before = head_position(&app, snake);

        run_fixed_update(&mut app);

        let after = head_position(&app, snake);
        assert_ne!(after, before);
        assert_eq!(
            app.world()
                .entity(snake)
                .get::<PatchHistory<TailPoints>>()
                .unwrap()
                .current_cursor(),
            None
        );
    }

    #[test]
    fn replicated_interpolated_remote_snake_does_not_move_without_authority() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.add_plugins(MovementPlugin);

        let snake = app
            .world_mut()
            .spawn((SnakeBundle::default(), Replicated, Interpolated))
            .id();
        let before = head_position(&app, snake);

        run_fixed_update(&mut app);

        let after = head_position(&app, snake);
        assert_eq!(after, before);
    }

    fn shorten_snake_entity(app: &mut App, snake: Entity) {
        let world = app.world_mut();
        let mut query = world.query::<(&SnakeHead, &mut TailPoints, &mut TailLength)>();
        let (head, mut tail, mut length) = query.get_mut(world, snake).unwrap();
        shorten_tail(head, &mut tail, &mut length);
    }

    #[test]
    fn test_shorten_tail() {
        let mut app = App::new();
        let snake = create_snake(&mut app);

        // shorten size
        app.world_mut()
            .entity_mut(snake)
            .get_mut::<TailLength>()
            .unwrap()
            .target_size = 130.0;
        shorten_snake_entity(&mut app, snake);

        // check that the tail has been shortened
        assert_eq!(
            visible_tail(&app, snake),
            TailPolyline::new(VecDeque::from(vec![
                (Vec2::new(50.0, 100.0), Direction::Right),
                (Vec2::new(0.0, 100.0), Direction::Right),
                (Vec2::new(0.0, 20.0), Direction::Up),
            ]))
        );
        assert_eq!(
            app.world()
                .entity(snake)
                .get::<TailLength>()
                .unwrap()
                .current_size,
            130.0
        );

        // shorten size again
        app.world_mut()
            .entity_mut(snake)
            .get_mut::<TailLength>()
            .unwrap()
            .target_size = 50.0;
        shorten_snake_entity(&mut app, snake);

        // check that the last point got removed
        assert_eq!(
            visible_tail(&app, snake),
            TailPolyline::new(VecDeque::from(vec![
                (Vec2::new(50.0, 100.0), Direction::Right),
                (Vec2::new(0.0, 100.0), Direction::Right),
            ]))
        );
        assert_eq!(
            app.world()
                .entity(snake)
                .get::<TailLength>()
                .unwrap()
                .current_size,
            50.0
        );

        // shorten size again
        app.world_mut()
            .entity_mut(snake)
            .get_mut::<TailLength>()
            .unwrap()
            .target_size = 30.0;
        shorten_snake_entity(&mut app, snake);

        // check that it works even with one segment
        assert_eq!(
            visible_tail(&app, snake),
            TailPolyline::new(VecDeque::from(vec![
                (Vec2::new(50.0, 100.0), Direction::Right),
                (Vec2::new(20.0, 100.0), Direction::Right),
            ]))
        );
        assert_eq!(
            app.world()
                .entity(snake)
                .get::<TailLength>()
                .unwrap()
                .current_size,
            30.0
        );
    }

    #[test]
    fn retained_authoritative_tail_keeps_extra_path_without_changing_visible_length() {
        let head = SnakeHead {
            position: Vec2::new(100.0, 100.0),
            direction: Direction::Right,
        };
        let mut tail = TailPoints::new(VecDeque::from([TailTurn::new(
            Vec2::new(0.0, 100.0),
            Direction::Down,
        )]));
        let mut length = TailLength {
            current_size: 100.0,
            target_size: 80.0,
        };

        shorten_tail_with_retention(&head, &mut tail, &mut length, 25.0);

        assert_eq!(length.current_size, 80.0);
        assert_eq!(tail.turns.len(), 1);

        length.target_size = 60.0;
        shorten_tail_with_retention(&head, &mut tail, &mut length, 25.0);

        assert_eq!(length.current_size, 60.0);
        assert!(tail.turns.is_empty());
    }
}
