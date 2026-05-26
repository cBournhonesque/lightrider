use crate::collision::collider::{snake_friction, SnakeFrictionEvent};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use lightyear::prelude::input::bei::Fire;

use crate::config::GameConfig;
use crate::network::protocol::components::snake::Direction;
use crate::network::protocol::prelude::*;
use crate::utils::query::Simulated;

pub struct MovementPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum SimulationSet {
    // move snakes
    Movement,
}

pub const MIN_SPEED: f32 = 1.0;
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
    mut query: Query<&mut TailPoints, Simulated>,
) {
    let Some(direction) = direction_from_input(trigger.value) else {
        return;
    };
    let Ok(mut tail) = query.get_mut(trigger.context) else {
        return;
    };
    turn_tail(&mut tail, direction);
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

pub fn turn_tail(tail: &mut TailPoints, requested: Direction) {
    let current = tail.front().1;
    let same_axis = matches!(
        (current, requested),
        (
            Direction::Up | Direction::Down,
            Direction::Up | Direction::Down
        ) | (
            Direction::Left | Direction::Right,
            Direction::Left | Direction::Right
        )
    );
    if !same_axis {
        tail.front_mut().1 = requested;
    }
}

// 2. update acceleration (are there close snakes?)
// - we start accelerating when we are close to another snake
// - otherwise we keep decelerating until we reach minimum speed
// - i'd like to add some easing; i.e have the change in acceleration not take place instantly
pub fn update_acceleration(
    mut events: MessageReader<SnakeFrictionEvent>,
    config: Res<GameConfig>,
    mut snakes: Query<(Entity, &mut Acceleration), Simulated>,
) {
    let movement = &config.movement;
    let mut accelerating_snakes = EntityHashSet::default();
    for event in events.read() {
        let Ok((_, mut acceleration)) = snakes.get_mut(event.main) else {
            continue;
        };
        accelerating_snakes.insert(event.main);
        acceleration.set_if_neq(Acceleration(boost_acceleration(
            movement.base_acceleration,
            movement.boost_acceleration_ratio,
            movement.boost_distance,
            event.distance,
        )));
    }
    // TODO: we'd like to add easing to this
    for (entity, mut acceleration) in snakes.iter_mut() {
        if !accelerating_snakes.contains(&entity) {
            acceleration.set_if_neq(Acceleration(movement.base_acceleration));
        }
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
    mut query: Query<(&mut TailPoints, &mut TailLength, &mut Speed, &Acceleration), Simulated>,
) {
    let movement = &config.movement;
    for (mut tail, mut length, mut speed, acceleration) in query.iter_mut() {
        // 3. update front of the tail: possibly add a new inflection point if necessary
        if tail.is_changed() {
            // copy the first point to the front when we have a turn
            let head = tail.0.front().unwrap().clone();
            tail.0.push_front(head);
        }

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
        tail.0
            .front_mut()
            .map(|(pos, dir)| *pos += dir.delta() * speed.0);
        length.current_size += speed.0;

        // 5. update the back of the tails: shorten tail
        // NOTE: it's ok to activate change detection here because we already updated the snake anyway
        shorten_tail(tail.as_mut(), length.as_mut());
    }
}

/// Shorten the tail to match the target size
pub fn shorten_tail(tail: &mut TailPoints, tail_length: &mut TailLength) {
    // if we still need to grow the tail, do nothing
    if tail_length.target_size >= tail_length.current_size {
        return;
    }

    // we need to shorten the tail
    let shorten_amount = tail_length.current_size - tail_length.target_size;
    tail.shorten_by(shorten_amount);
    tail_length.current_size = tail_length.target_size;
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    fn create_snake(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                TailPoints(VecDeque::from(vec![
                    (Vec2::new(50.0, 100.0), Direction::Right),
                    (Vec2::new(0.0, 100.0), Direction::Right),
                    (Vec2::new(0.0, 0.0), Direction::Up),
                ])),
                TailLength {
                    current_size: 150.0,
                    target_size: 150.0,
                },
                Speed(0.5),
                Acceleration(0.0),
            ))
            .id()
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
        let mut tail = TailPoints(VecDeque::from(vec![
            (Vec2::ZERO, Direction::Up),
            (Vec2::new(0.0, -10.0), Direction::Up),
        ]));

        turn_tail(&mut tail, Direction::Down);
        assert_eq!(tail.front().1, Direction::Up);

        turn_tail(&mut tail, Direction::Right);
        assert_eq!(tail.front().1, Direction::Right);

        turn_tail(&mut tail, Direction::Left);
        assert_eq!(tail.front().1, Direction::Right);
    }

    #[test]
    fn boost_acceleration_scales_with_distance() {
        assert!((boost_acceleration(-0.01, 2.0, 20.0, 20.0) - 0.0).abs() < f32::EPSILON);
        assert!((boost_acceleration(-0.01, 2.0, 20.0, 0.0) - 0.02).abs() < f32::EPSILON);
        assert!((boost_acceleration(-0.01, 2.0, 20.0, 10.0) - 0.01).abs() < f32::EPSILON);
    }

    fn shorten_snake_entity(app: &mut App, snake: Entity) {
        let world = app.world_mut();
        let mut query = world.query::<(&mut TailPoints, &mut TailLength)>();
        let (mut tail, mut length) = query.get_mut(world, snake).unwrap();
        shorten_tail(&mut tail, &mut length);
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
            app.world().entity(snake).get::<TailPoints>().unwrap(),
            &TailPoints(VecDeque::from(vec![
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
            app.world().entity(snake).get::<TailPoints>().unwrap(),
            &TailPoints(VecDeque::from(vec![
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
            app.world().entity(snake).get::<TailPoints>().unwrap(),
            &TailPoints(VecDeque::from(vec![
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
}
