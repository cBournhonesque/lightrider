use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::*;
use crate::network::protocol::prelude::*;
use crate::network::protocol::components::snake::Direction;
use crate::utils::query::Controlled;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // 1. turn heads if we received inputs -> done automatically during replication
        // 2. update acceleration (are there close snakes?)
        // 3. update the front of the tail: possibly add a new inflection point if necessary (if direction changed)
        // 4. update heads: integrate acceleration into velocity, integrate velocity into position
        // 5. update the back of the tails: shorten tail
        app.add_systems(FixedUpdate,
                        (turn_heads, update_tails_front, update_heads, update_tails_back)
                            .chain()
                            .in_set(FixedUpdateSet::Main)
        );
    }
}

// 1. turn heads according to input
pub fn turn_heads(
    mut query: Query<(&mut HeadDirection, &ActionState<PlayerMovement>), Controlled>,
) {
    for (mut direction, input) in query.iter_mut() {
        if input.pressed(PlayerMovement::Up) {
            if direction.0 != Direction::Down && direction.0 != Direction::Up {
                direction.0 = Direction::Up;
            }
        } else if input.pressed(PlayerMovement::Down) {
            if direction.0 != Direction::Down && direction.0 != Direction::Up {
                direction.0 = Direction::Down;
            }
        } else if input.pressed(PlayerMovement::Left) {
            if direction.0 != Direction::Left && direction.0 != Direction::Right {
                direction.0 = Direction::Left;
            }
        } else if input.pressed(PlayerMovement::Right) {
            if direction.0 != Direction::Left && direction.0 != Direction::Right {
                direction.0 = Direction::Right;
            }
        }
    }
}

// 3. update front of the tail: possibly add a new inflection point if necessary
pub fn update_tails_front(
    heads: Query<(&HeadPoint, Ref<HeadDirection>), Controlled>,
    mut query: Query<(&mut TailPoints, &TailParent), Controlled>
) {
    for (mut tail, parent) in query.iter_mut() {
        let Ok((head, direction)) = heads.get(parent.0) else {
            error!("Update tails front: Snake tail has no parent head: {:?}", parent.0);
            continue;
        };

        // if direction changed, we need to add a new point to the tail
        if direction.is_changed() {
            // it's just the current head position (we haven't updated it yet)
            tail.0.push_front((head.0, direction.0.clone()));
        }
    }
}

// 4. update heads' speed and position
pub fn update_heads(
    mut query: Query<(&mut HeadPoint, &mut TailLength, &HeadDirection, &mut Speed, &Acceleration), Controlled>
) {
    for (mut position, mut length, direction, mut speed, acceleration) in query.iter_mut() {
        // update velocity
        speed.0 += acceleration.0;

        // update position
        position.0 += direction.0.delta() * speed.0;
        length.current_size += speed.0;
    }
}


// 5. update the back of the tails: shorten tail
pub fn update_tails_back(
    mut heads: Query<(&HeadPoint, &mut TailLength), Controlled>,
    mut query: Query<(&mut TailPoints, &TailParent), Controlled>
) {
    for (mut tail, parent) in query.iter_mut() {
        let Ok((head, mut length)) = heads.get_mut(parent.0) else {
            error!("Update tails back: Snake tail has no parent head: {:?}", parent.0);
            continue;
        };

        // if we still need to grow the tail, do nothing
        if length.target_size >= length.current_size {
            continue;
        }

        // we need to shorten the tail
        let mut shorten_amount = length.current_size - length.target_size;
        // iterate from the tail to the front
        let mut drop_point = 0;
        let mut new_point = None;
        // the direction isn't used so we just use Up
        for (i, (from, to)) in tail.pairs_back_to_front(&(head.0, Direction::Up)).enumerate() {
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

        length.current_size = length.target_size;
    }
}

/// Shorten the tail to match the target size
pub fn shorten_tail(tail: &mut TailPoints, head: &HeadPoint, tail_length: &mut TailLength) {
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
    for (i, (from, to)) in tail.pairs_back_to_front(&(head.0, Direction::Up)).enumerate() {
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

    fn create_snake(app: &mut App) -> (Entity, Entity) {
        let head = app.world.spawn(
            (
                HeadPoint(Vec2::new(50.0, 100.0)),
                HeadDirection(Direction::Right),
                TailLength { current_size: 150.0, target_size: 150.0 },
                Speed(0.5),
                Acceleration(0.0),
            )
        ).id();
        let tail = app.world.spawn(
            (
                TailPoints(VecDeque::from(vec![
                    (Vec2::new(0.0, 100.0), Direction::Right),
                    (Vec2::new(0.0, 0.0), Direction::Up),
                ])),
                TailParent(head),
            )
        ).id();
        (head, tail)
    }

    #[test]
    fn test_shorten_tail() {
        let mut app = App::new();
        app.add_systems(Update, update_tails_back);
        let (head, tail) = create_snake(&mut app);

        // shorten size
        app.world.entity_mut(head).get_mut::<TailLength>().unwrap().target_size = 130.0;
        app.update();

        // check that the tail has been shortened
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap(),
                     &TailPoints(VecDeque::from(vec![
                          (Vec2::new(0.0, 100.0), Direction::Right),
                          (Vec2::new(0.0, 20.0), Direction::Up),
                     ])));
        assert_eq!(app.world.entity(head).get::<TailLength>().unwrap().current_size, 130.0);

        // shorten size again
        app.world.entity_mut(head).get_mut::<TailLength>().unwrap().target_size = 50.0;
        app.update();

        // check that the last point got removed
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap(),
                   &TailPoints(VecDeque::from(vec![
                       (Vec2::new(0.0, 100.0), Direction::Right),
                   ])));
        assert_eq!(app.world.entity(head).get::<TailLength>().unwrap().current_size, 50.0);

        // shorten size again
        app.world.entity_mut(head).get_mut::<TailLength>().unwrap().target_size = 30.0;
        app.update();

        // check that it works even with one segment
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap(),
                   &TailPoints(VecDeque::from(vec![
                       (Vec2::new(20.0, 100.0), Direction::Right),
                   ])));
        assert_eq!(app.world.entity(head).get::<TailLength>().unwrap().current_size, 30.0);
    }

}



