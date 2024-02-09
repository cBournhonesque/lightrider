use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::*;

use crate::network::protocol::components::snake::Direction;
use crate::network::protocol::prelude::*;
use crate::utils::query::Controlled;

pub struct MovementPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum SimulationSet {
    // move snakes
    Movement,
}


impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {

        app.configure_sets(FixedUpdate, SimulationSet::Movement.in_set(FixedUpdateSet::Main));

        // 1. turn heads if we received inputs -> done automatically during replication
        // 2. update acceleration (are there close snakes?)
        // 3. update the front of the tail: possibly add a new inflection point if necessary (if direction changed)
        // 4. update heads: integrate acceleration into velocity, integrate velocity into position
        // 5. update the back of the tails: shorten tail
        app.add_systems(FixedUpdate,
                        (turn_heads, update_tails)
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

// 2. update acceleration (are there close snakes?)
// TODO

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
        speed.0 += acceleration.0;

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
    let mut shorten_amount = tail_length.current_size - tail_length.target_size;
    // iterate from the tail to the front
    let mut drop_point = 0;
    let mut new_point = None;
    // the direction isn't used so we just use Up
    for (i, (from, to)) in tail.pairs_back_to_front().enumerate() {
        let segment_size = from.0.distance(to.0);

        if segment_size >= shorten_amount {
            // we need to shorten this segment, and drop all the points past that
            drop_point = tail.0.len() - 1 - i;
            if segment_size > shorten_amount {
                new_point = Some(from.0 + from.1.delta() * shorten_amount);
            }
            break;
        } else {
            // we still need to shorten more
            shorten_amount -= segment_size;
        }
    }

    // drop the tail points
    let drained = tail.0.drain(drop_point..).next().unwrap();
    // add the new point
    if let Some(new_point) = new_point {
        tail.0.push_back((new_point, drained.1));
    }

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



