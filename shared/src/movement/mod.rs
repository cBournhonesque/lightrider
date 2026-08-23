use crate::collision::collider::{snake_friction, SnakeFrictionEvent};
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use bevy_replicon::prelude::{EntityCommandsDiffExt, ReplicationStorage};
use lightyear::prelude::input::bei::Fire;

use crate::config::GameConfig;
use crate::network::protocol::components::snake::Direction;
use crate::network::protocol::prelude::*;
use crate::utils::query::{Simulated, SimulationAuthority};
use lightyear::prelude::LocalTimeline;

pub struct MovementPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum SimulationSet {
    // move snakes
    Movement,
}

pub const MIN_SPEED: f32 = 0.85;
pub const MAX_SPEED: f32 = 2.45;
pub const BASE_ACCELERATION: f32 = -0.01;
pub const ACCELERATION_RATIO: f32 = 1.35;
const TURN_RATE_LIMIT_HISTORY: usize = 16;
pub const TURN_RATE_LIMIT_WINDOW_SECONDS: f32 = 1.0;

#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct TurnRateLimiter {
    recent_turn_ticks: [u32; TURN_RATE_LIMIT_HISTORY],
    recent_turn_count: u8,
}

impl Default for TurnRateLimiter {
    fn default() -> Self {
        Self {
            recent_turn_ticks: [0; TURN_RATE_LIMIT_HISTORY],
            recent_turn_count: 0,
        }
    }
}

impl TurnRateLimiter {
    pub fn try_consume_turn(
        &mut self,
        current_tick: u32,
        max_turns: u8,
        window_ticks: u32,
    ) -> bool {
        if max_turns == 0 {
            return false;
        }
        self.prune(current_tick, window_ticks.max(1));
        if self
            .recent_turn_ticks
            .iter()
            .take(usize::from(self.recent_turn_count))
            .any(|tick| *tick == current_tick)
        {
            return false;
        }
        let max_turns = usize::from(max_turns).min(TURN_RATE_LIMIT_HISTORY);
        if usize::from(self.recent_turn_count) >= max_turns {
            return false;
        }
        self.recent_turn_ticks[usize::from(self.recent_turn_count)] = current_tick;
        self.recent_turn_count += 1;
        true
    }

    fn prune(&mut self, current_tick: u32, window_ticks: u32) {
        let mut kept = 0;
        for index in 0..usize::from(self.recent_turn_count) {
            let turn_tick = self.recent_turn_ticks[index];
            if turn_tick <= current_tick && current_tick.saturating_sub(turn_tick) < window_ticks {
                self.recent_turn_ticks[kept] = turn_tick;
                kept += 1;
            }
        }
        self.recent_turn_count = kept as u8;
    }
}

pub fn turn_rate_limit_window_ticks(config: &GameConfig) -> u32 {
    turn_rate_limit_window_ticks_for_rate(config.movement.tick_rate_hz)
}

fn turn_rate_limit_window_ticks_for_rate(tick_rate_hz: f32) -> u32 {
    (tick_rate_hz.max(1.0) * TURN_RATE_LIMIT_WINDOW_SECONDS)
        .round()
        .max(1.0) as u32
}

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // events
        app.add_message::<SnakeFrictionEvent>();
        app.init_resource::<ReplicationStorage>();

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
    mut commands: Commands,
    mut query: Query<(Entity, &mut SnakeHead, &TailPoints), Simulated>,
) {
    let Some(direction) = direction_from_input(trigger.value) else {
        return;
    };
    let Ok((entity, mut head, _tail)) = query.get_mut(trigger.context) else {
        return;
    };
    let current = head.direction;
    if !is_perpendicular_turn(current, direction) {
        return;
    }

    commands
        .entity(entity)
        .apply_diff::<TailPoints>(TailPointsDiff::PushTurn(TailTurn::new(
            head.position,
            current.opposite(),
        )));
    head.direction = direction;
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
    let current = head.direction;
    if is_perpendicular_turn(current, requested) {
        tail.apply_topology_diff(TailPointsDiff::PushTurn(TailTurn::new(
            head.position,
            current.opposite(),
        )));
        head.direction = requested;
    }
}

pub fn turn_tail_with_diff(
    commands: &mut Commands,
    entity: Entity,
    head: &mut SnakeHead,
    requested: Direction,
) {
    let current = head.direction;
    if is_perpendicular_turn(current, requested) {
        commands
            .entity(entity)
            .apply_diff::<TailPoints>(TailPointsDiff::PushTurn(TailTurn::new(
                head.position,
                current.opposite(),
            )));
        head.direction = requested;
    }
}

pub fn turn_tail_with_diff_limited(
    commands: &mut Commands,
    entity: Entity,
    head: &mut SnakeHead,
    requested: Direction,
    limiter: &mut TurnRateLimiter,
    current_tick: u32,
    max_turns: u8,
    window_ticks: u32,
) -> bool {
    if !is_perpendicular_turn(head.direction, requested) {
        return false;
    }
    if !limiter.try_consume_turn(current_tick, max_turns, window_ticks) {
        return false;
    }
    turn_tail_with_diff(commands, entity, head, requested);
    true
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
        let target_acceleration = combined_acceleration(
            movement.base_acceleration,
            proximity_boosts.get(&entity).copied(),
            food_boost.0,
        );
        acceleration.set_if_neq(Acceleration(smoothed_acceleration(
            acceleration.0,
            target_acceleration,
            movement.acceleration_smoothing,
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

pub fn smoothed_acceleration(current: f32, target: f32, smoothing: f32) -> f32 {
    current + (target - current) * smoothing.clamp(0.0, 1.0)
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
    mut commands: Commands,
    config: Res<GameConfig>,
    timeline: Option<Res<LocalTimeline>>,
    mut query: Query<
        (
            Entity,
            &mut SnakeHead,
            &TailPoints,
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
        entity,
        mut head,
        tail,
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
        shorten_tail_with_retention_with_diff(
            &mut commands,
            entity,
            head.as_ref(),
            tail,
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
    if tail_length.target_size < tail_length.current_size {
        tail_length.current_size = tail_length.target_size;
    }

    let retained_length = tail_length.current_size + retained_extra_length.max(0.0);
    let _ = tail.prune_to_length(head, retained_length);
}

fn shorten_tail_with_retention_with_diff(
    commands: &mut Commands,
    entity: Entity,
    head: &SnakeHead,
    tail: &TailPoints,
    tail_length: &mut TailLength,
    retained_extra_length: f32,
) -> usize {
    if tail_length.target_size < tail_length.current_size {
        tail_length.current_size = tail_length.target_size;
    }

    let retained_length = tail_length.current_size + retained_extra_length.max(0.0);
    let removed = tail_turns_to_remove(head, tail, retained_length);
    if removed > 0 {
        let diff = TailPointsDiff::RemoveTailTurns(removed.min(u16::MAX as usize) as u16);
        commands.entity(entity).apply_diff::<TailPoints>(diff);
    }
    removed
}

fn tail_turns_to_remove(head: &SnakeHead, tail: &TailPoints, retained_length: f32) -> usize {
    let retained_length = retained_length.max(0.0);
    let mut distance = 0.0;
    let mut previous = head.position;
    let mut keep = tail.turns.len();

    for (index, turn) in tail.turns.iter().enumerate() {
        distance += previous.distance(turn.position);
        if distance >= retained_length - f32::EPSILON {
            keep = index;
            break;
        }
        previous = turn.position;
    }

    tail.turns.len().saturating_sub(keep)
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
    use bevy_enhanced_input::prelude::TriggerState;
    use bevy_replicon::prelude::DiffIndex;
    use bevy_replicon::shared::replication::diff::DiffHistory;
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

    fn force_tail_prune(app: &mut App, snake: Entity, target_size: f32) {
        app.world_mut()
            .entity_mut(snake)
            .get_mut::<TailLength>()
            .unwrap()
            .target_size = target_size;
    }

    fn assert_tail_diff_recorded(app: &App, snake: Entity) {
        let storage = app.world().resource::<ReplicationStorage>();
        let history = storage.get::<DiffHistory<TailPoints>>(snake).unwrap();
        assert_eq!(history.current_index(), DiffIndex::new(0));
    }

    fn fire_turn(app: &mut App, snake: Entity, action: Entity, direction: Direction) {
        app.world_mut().trigger(Fire::<MoveSnake> {
            context: snake,
            action,
            value: direction.delta(),
            state: TriggerState::Fired,
            fired_secs: 0.0,
            elapsed_secs: 0.0,
        });
        app.world_mut().flush();
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
    fn move_snake_fire_turns_head_and_records_tail_diff() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(MovementPlugin);

        let snake = app
            .world_mut()
            .spawn((SnakeBundle::default(), Predicted))
            .id();
        let action = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(Fire::<MoveSnake> {
            context: snake,
            action,
            value: Vec2::X,
            state: TriggerState::Fired,
            fired_secs: 0.0,
            elapsed_secs: 0.0,
        });
        app.world_mut().flush();

        let entity = app.world().entity(snake);
        assert_eq!(
            entity.get::<SnakeHead>().unwrap().direction,
            Direction::Right
        );
        assert_eq!(
            entity.get::<TailPoints>().unwrap().turns,
            VecDeque::from([TailTurn::new(Vec2::ZERO, Direction::Down)])
        );
        assert_tail_diff_recorded(&app, snake);
    }

    #[test]
    fn move_snake_fire_does_not_apply_bot_turn_limiter() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(MovementPlugin);

        let snake = app
            .world_mut()
            .spawn((SnakeBundle::default(), Predicted))
            .id();
        let action = app.world_mut().spawn_empty().id();
        let mut current = Direction::Up;

        for _ in 0..=GameConfig::default().bots.max_turns_per_second {
            let requested = match current {
                Direction::Up | Direction::Down => Direction::Right,
                Direction::Left | Direction::Right => Direction::Up,
            };
            fire_turn(&mut app, snake, action, requested);
            current = requested;
            assert_eq!(
                app.world()
                    .entity(snake)
                    .get::<SnakeHead>()
                    .unwrap()
                    .direction,
                current
            );
        }
    }

    #[test]
    fn bot_turn_limiter_rejects_sixth_turn_in_one_second_window() {
        let mut limiter = TurnRateLimiter::default();
        let config = GameConfig::default();
        let max_turns = config.bots.max_turns_per_second;
        let window_ticks = turn_rate_limit_window_ticks(&config);

        for tick in 1..=u32::from(max_turns) {
            assert!(limiter.try_consume_turn(tick, max_turns, window_ticks));
        }

        assert!(!limiter.try_consume_turn(u32::from(max_turns) + 1, max_turns, window_ticks));
        assert!(!limiter.try_consume_turn(u32::from(max_turns), max_turns, window_ticks));
        assert!(limiter.try_consume_turn(window_ticks + 1, max_turns, window_ticks));
    }

    #[test]
    fn limited_turn_commit_rejects_sixth_tail_diff_in_one_second_window() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(MovementPlugin);

        let snake = app.world_mut().spawn(TailPoints::empty()).id();
        let config = GameConfig::default();
        let max_turns = config.bots.max_turns_per_second;
        let window_ticks = turn_rate_limit_window_ticks(&config);
        let mut limiter = TurnRateLimiter::default();
        let mut head = SnakeHead::default();

        for tick in 1..=u32::from(max_turns) {
            let requested = match head.direction {
                Direction::Up | Direction::Down => Direction::Right,
                Direction::Left | Direction::Right => Direction::Up,
            };
            let accepted = {
                let mut commands = app.world_mut().commands();
                turn_tail_with_diff_limited(
                    &mut commands,
                    snake,
                    &mut head,
                    requested,
                    &mut limiter,
                    tick,
                    max_turns,
                    window_ticks,
                )
            };
            app.world_mut().flush();
            assert!(accepted);
            assert_eq!(head.direction, requested);
        }

        let blocked = match head.direction {
            Direction::Up | Direction::Down => Direction::Right,
            Direction::Left | Direction::Right => Direction::Up,
        };
        let accepted = {
            let mut commands = app.world_mut().commands();
            turn_tail_with_diff_limited(
                &mut commands,
                snake,
                &mut head,
                blocked,
                &mut limiter,
                u32::from(max_turns) + 1,
                max_turns,
                window_ticks,
            )
        };
        app.world_mut().flush();

        assert!(!accepted);
        assert_ne!(head.direction, blocked);
        assert_eq!(
            app.world()
                .entity(snake)
                .get::<TailPoints>()
                .unwrap()
                .turns
                .len(),
            usize::from(max_turns)
        );
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
    fn acceleration_smoothing_moves_toward_target() {
        assert_eq!(smoothed_acceleration(0.0, 1.0, 0.0), 0.0);
        assert_eq!(smoothed_acceleration(0.0, 1.0, 1.0), 1.0);
        assert_eq!(smoothed_acceleration(0.0, 1.0, 0.25), 0.25);
        assert_eq!(smoothed_acceleration(1.0, 0.0, 0.25), 0.75);
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
    fn authoritative_snake_tail_prunes_via_diff() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.add_plugins(MovementPlugin);

        let snake = create_snake(&mut app);
        app.world_mut()
            .entity_mut(snake)
            .insert(SimulationAuthority);
        force_tail_prune(&mut app, snake, 10.0);

        run_fixed_update(&mut app);

        assert!(app
            .world()
            .entity(snake)
            .get::<TailPoints>()
            .unwrap()
            .turns
            .is_empty());
        assert_tail_diff_recorded(&app, snake);
    }

    #[test]
    fn predicted_snake_tail_prunes_via_diff() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.add_plugins(MovementPlugin);

        let snake = create_snake(&mut app);
        app.world_mut().entity_mut(snake).insert(Predicted);
        force_tail_prune(&mut app, snake, 10.0);
        let before = head_position(&app, snake);

        run_fixed_update(&mut app);

        let after = head_position(&app, snake);
        assert_ne!(after, before);
        assert!(app
            .world()
            .entity(snake)
            .get::<TailPoints>()
            .unwrap()
            .turns
            .is_empty());
        assert_tail_diff_recorded(&app, snake);
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
