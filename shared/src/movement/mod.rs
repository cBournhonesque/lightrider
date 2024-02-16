use bevy::prelude::*;
use bevy::utils::EntityHashSet;
use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::*;
use crate::collision::collider::{MAX_FRICTION_DISTANCE, snake_friction, SnakeFrictionEvent};

use crate::network::protocol::components::snake::Direction;
use crate::network::protocol::prelude::*;
use crate::utils::query::Controlled;

pub struct MovementPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum SimulationSet {
    // move snakes
    Movement,
}

pub const MIN_SPEED: f32 = 1.0;
pub const MAX_SPEED: f32 = 4.0;


impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // events
        app.add_event::<SnakeFrictionEvent>();

        // sets
        app.configure_sets(FixedUpdate, SimulationSet::Movement.in_set(FixedUpdateSet::Main));

        // 1. turn heads if we received inputs -> done automatically during replication
        // 2. update acceleration (are there close snakes?)
        // 3. update the front of the tail: possibly add a new inflection point if necessary (if direction changed)
        // 4. update heads: integrate acceleration into velocity, integrate velocity into position
        // 5. update the back of the tails: shorten tail
        app.add_systems(FixedUpdate,
                        (turn_heads, update_acceleration.after(snake_friction), update_tails)
                            .chain()
                            .in_set(SimulationSet::Movement)
        );
    }
}

// 1. turn heads according to input
pub fn turn_heads(
    mut query: Query<(&mut TailPoints, &ActionState<PlayerMovement>), Controlled>,
) {
    for (mut tail, input) in query.iter_mut() {
        let direction = tail.0.front().unwrap().1;
        if input.pressed(PlayerMovement::Up) {
            if direction != Direction::Down && direction != Direction::Up {
                tail.0.front_mut().unwrap().1 = Direction::Up;
            }
        } else if input.pressed(PlayerMovement::Down) {
            if direction != Direction::Down && direction != Direction::Up {
                tail.0.front_mut().unwrap().1 = Direction::Down;
            }
        } else if input.pressed(PlayerMovement::Left) {
            if direction != Direction::Left && direction != Direction::Right {
                tail.0.front_mut().unwrap().1 = Direction::Left;
            }
        } else if input.pressed(PlayerMovement::Right) {
            if direction != Direction::Left && direction != Direction::Right {
                tail.0.front_mut().unwrap().1 = Direction::Right;
            }
        }
    }
}

pub const BASE_ACCELERATION: f32 = -0.01;
pub const ACCELERATION_RATIO: f32 = 2.0;

// 2. update acceleration (are there close snakes?)
// - we start accelerating when we are close to another snake
// - otherwise we keep decelerating until we reach minimum speed
// - i'd like to add some easing; i.e have the change in acceleration not take place instantly
pub fn update_acceleration(
    mut events: EventReader<SnakeFrictionEvent>,
    mut snakes: Query<(Entity, &mut Acceleration), Controlled>
) {
    let mut accelerating_snakes = EntityHashSet::default();
    for event in events.read() {
        let Ok((_, mut acceleration)) = snakes.get_mut(event.main) else {
            continue;
        };
        accelerating_snakes.insert(event.main);
        acceleration.set_if_neq(Acceleration(BASE_ACCELERATION.abs() * ACCELERATION_RATIO * (MAX_FRICTION_DISTANCE - event.distance) / MAX_FRICTION_DISTANCE));
    }
    // TODO: we'd like to add easing to this
    for (entity, mut acceleration) in snakes.iter_mut() {
        if !accelerating_snakes.contains(&entity) {
            acceleration.set_if_neq(Acceleration(BASE_ACCELERATION));
        }
    }
}

// 3. update front of the tail: possibly add a new inflection point if necessary
// 4. update acceleration and speed
// 5. update the back of the tails: shorten tail
pub fn update_tails(
    mut query: Query<(&mut TailPoints, &mut TailLength, &mut Speed, &Acceleration), Controlled>
) {
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
        if !((acceleration.0 < 0.0 && speed.0 == MIN_SPEED) || (acceleration.0 > 0.0 && speed.0 == MAX_SPEED)) {
            speed.0 += acceleration.0;
            speed.0 = speed.0.max(MIN_SPEED).min(MAX_SPEED);
        }

        // update position
        tail.0.front_mut().map(|(pos, dir)| *pos += dir.delta() * speed.0);
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
        app.world.spawn(
            (
                TailPoints(VecDeque::from(vec![
                    (Vec2::new(50.0, 100.0), Direction::Right),
                    (Vec2::new(0.0, 100.0), Direction::Right),
                    (Vec2::new(0.0, 0.0), Direction::Up),
                ])),
                TailLength { current_size: 150.0, target_size: 150.0 },
                Speed(0.5),
                Acceleration(0.0),
            )
        ).id()
    }

    #[test]
    fn test_shorten_tail() {
        let mut app = App::new();
        let snake = create_snake(&mut app);

        // shorten size
        app.world.entity_mut(snake).get_mut::<TailLength>().unwrap().target_size = 130.0;
        let (mut tail, mut length) = app.world.query::<(&mut TailPoints, &mut TailLength)>().get_mut(&mut app.world, snake).unwrap();
        shorten_tail(&mut tail, &mut length);

        // check that the tail has been shortened
        assert_eq!(app.world.entity(snake).get::<TailPoints>().unwrap(),
                     &TailPoints(VecDeque::from(vec![
                         (Vec2::new(50.0, 100.0), Direction::Right),
                          (Vec2::new(0.0, 100.0), Direction::Right),
                          (Vec2::new(0.0, 20.0), Direction::Up),
                     ])));
        assert_eq!(app.world.entity(snake).get::<TailLength>().unwrap().current_size, 130.0);

        // shorten size again
        app.world.entity_mut(snake).get_mut::<TailLength>().unwrap().target_size = 50.0;
        let (mut tail, mut length) = app.world.query::<(&mut TailPoints, &mut TailLength)>().get_mut(&mut app.world, snake).unwrap();
        shorten_tail(&mut tail, &mut length);

        // check that the last point got removed
        assert_eq!(app.world.entity(snake).get::<TailPoints>().unwrap(),
                   &TailPoints(VecDeque::from(vec![
                       (Vec2::new(50.0, 100.0), Direction::Right),
                       (Vec2::new(0.0, 100.0), Direction::Right),
                   ])));
        assert_eq!(app.world.entity(snake).get::<TailLength>().unwrap().current_size, 50.0);

        // shorten size again
        app.world.entity_mut(snake).get_mut::<TailLength>().unwrap().target_size = 30.0;
        let (mut tail, mut length) = app.world.query::<(&mut TailPoints, &mut TailLength)>().get_mut(&mut app.world, snake).unwrap();
        shorten_tail(&mut tail, &mut length);

        // check that it works even with one segment
        assert_eq!(app.world.entity(snake).get::<TailPoints>().unwrap(),
                   &TailPoints(VecDeque::from(vec![
                       (Vec2::new(50.0, 100.0), Direction::Right),
                       (Vec2::new(20.0, 100.0), Direction::Right),
                   ])));
        assert_eq!(app.world.entity(snake).get::<TailLength>().unwrap().current_size, 30.0);
    }
}



