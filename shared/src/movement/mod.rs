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

// 3. update front of the tail: possibly add a new inflection point if necessary
pub fn update_tails_front(
    heads: Query<(&HeadPoint, Ref<HeadDirection>), Controlled>,
    mut query: Query<(&mut TailPoints, &Parent),  Controlled>
) {
    for (mut tail, parent) in query.iter_mut() {
        let Ok((head, direction)) = heads.get(parent.get()) else {
            error!("Snake tail has no parent head");
            continue;
        };

        // if direction changed, we need to add a new point to the tail
        if direction.is_changed() {
            info!("adding new tail point");
            // it's just the current head position (we haven't updated it yet)
            tail.0.push_front((head.0, direction.0.clone()));
        }
    }
}

// 5. update the back of the tails: shorten tail
pub fn update_tails_back(
    heads: Query<(&HeadPoint, &TailLength, Ref<HeadDirection>), Controlled>,
    mut query: Query<(&mut TailPoints, &Parent),  Controlled>
) {
    for (mut tail, parent) in query.iter_mut() {
        let Ok((head, length, direction)) = heads.get(parent.get()) else {
            error!("Snake tail has no parent head");
            continue;
        };

        // if we still need to grow the tail, do nothing
        if length.target_size >= length.current_size {
            continue;
        }

        // TODO: add unit tests

        // we need to shorten the tail
        let mut shorten_amount = length.current_size - length.target_size;
        // iterate from the tail to the front
        let drop_point;
        let mut new_point = None;
        for (i, (from, to)) in tail.pairs(&(head.0, direction.0)).enumerate().rev() {
            let segment_size = from.0.distance(to.0);

            if segment_size == shorten_amount {
                drop_point = i;
            } else if segment_size > shorten_amount {
                // we need to shorten this segment, and drop all the points past that
                new_point = Some(from.0 + from.1.delta() * shorten_amount);
                drop_point = i;
            } else {
                // we still need to shorten more
                shorten_amount -= segment_size;
            }
        }

        // drop the tail points
        let mut drained = tail.0.drain(drop_point..);
        // add the new point
        if let Some(new_point) = new_point {
            tail.0.push_back((new_point, drained.next().unwrap().1));
        }
    }
}



