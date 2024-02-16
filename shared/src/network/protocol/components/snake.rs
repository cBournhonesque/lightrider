use std::collections::VecDeque;

use bevy::prelude::*;
use derive_more::{Add, Mul};
use itertools::Itertools;
use lightyear::prelude::*;
use lightyear::prelude::client::LerpFn;
use parry2d::math::Point;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn delta(&self) -> Vec2 {
        match self {
            Direction::Left => Vec2::new(-1.0, 0.0),
            Direction::Right => Vec2::new(1.0, 0.0),
            Direction::Up => Vec2::new(0.0, 1.0),
            Direction::Down => Vec2::new(0.0, -1.0),
        }
    }
}


#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
pub struct TailLength{
    pub current_size: f32,
    pub target_size: f32,
}

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
// tail inflection points, from front (head point) to back (tail end point)
pub struct TailPoints(pub VecDeque<(Vec2, Direction)>);

// TODO: replace this with Parent in bevy 0.13
#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
#[message(custom_map)]
pub struct HasPlayer(pub Entity);

impl<'a> MapEntities<'a> for HasPlayer {
    fn map_entities(&mut self, entity_mapper: Box<dyn EntityMapper + 'a>) {
        self.0.map_entities(entity_mapper);
    }

    fn entities(&self) -> bevy::utils::EntityHashSet<Entity> {
        bevy::utils::EntityHashSet::from_iter(vec![self.0])
    }
}

impl TailPoints {

    pub fn front(&self) -> &(Vec2, Direction) {
        self.0.front().unwrap()
    }

    pub fn front_mut(&mut self) -> &mut (Vec2, Direction) {
        self.0.front_mut().unwrap()
    }
    pub fn pairs_front_to_back<'a>(&'a self) -> impl Iterator<Item = (&(Vec2, Direction), &(Vec2, Direction))> {
        self.0.iter().tuple_windows().map(|(a, b)| (b, a))
    }

    pub fn pairs_back_to_front<'a>(&'a self) -> impl Iterator<Item = (&(Vec2, Direction), &(Vec2, Direction))> {
        self.0.iter().rev().tuple_windows()
    }

    pub fn points_front_to_back(&self) -> Vec<Point<f32>> {
        self.0.iter().map(|(v, _)| Point::new(v.x, v.y)).collect()
    }

    /// Shorten the tail by a certain amount
    pub fn shorten_by(&mut self, mut shorten_amount: f32) {
        // iterate from the tail to the front
        let mut drop_point = 0;
        let mut new_point = None;
        // the direction isn't used so we just use Up
        for (i, (from, to)) in self.pairs_back_to_front().enumerate() {
            let segment_size = from.0.distance(to.0);

            if segment_size >= shorten_amount {
                // we need to shorten this segment, and drop all the points past that
                drop_point = self.0.len() - 1 - i;
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
        let drained = self.0.drain(drop_point..).next().unwrap();
        // add the new point
        if let Some(new_point) = new_point {
            self.0.push_back((new_point, drained.1));
        }
    }
}

/// Perform the interpolation of snake points
pub struct SnakeInterpolator;

impl LerpFn<TailPoints> for SnakeInterpolator {
    fn lerp(start: &TailPoints, end: &TailPoints, t: f32) -> TailPoints {
        let mut tail =  start.clone();
        // distance between the two heads while remaining on the tail path
        let mut tail_diff_length = 0.0;

        // distance that we need to move the head point while remaining on the tail path
        let mut pos_distance_to_move = 0.0;
        // segment in which the starting pos is (0 is [front_tail -> head])
        let mut segment_idx = usize::MAX;

        // 1. we need to find in which end tail segment the start head is, and the difference in length
        //    between the two tails
        for (i, (from, to)) in end.pairs_front_to_back().enumerate() {
            // we found the segment on which the head point is
            if crate::utils::geometry::segment_contains_point(&from.0, &to.0, &tail.front().0) {
                tail_diff_length += to.0.distance(tail.front().0);
                // if the head point is at a turn point, we need to add a turn point right now before we move the head point
                // in the later stage (only if it's actually turning!)
                if tail.front().0 == from.0 && tail.front().1 != from.1 {
                    tail.front_mut().1 = from.1;
                    tail.0.push_front(from.clone());
                }
                pos_distance_to_move = t * tail_diff_length;
                segment_idx = i;
                break;
            } else {
                tail_diff_length += from.0.distance(to.0);
            }
        }
        if pos_distance_to_move == 0.0 {
            return tail;
        }
        if segment_idx == usize::MAX {
            // the difference between start/end is bigger than the length of the snake
            panic!("could not find segment on which the head point is");
        }

        // 2. now move the head point by `pos_distance_to_move` while remaining on the end tail path
        // length.current_size += pos_distance_to_move;
        for (from, to) in end.pairs_back_to_front().skip(end.0.len() - 2 - segment_idx) {
            let dist = tail.front().0.distance(to.0);
            // the head tail has to go to the next segment
            if dist <= pos_distance_to_move {
                // move the front of the tail to the end of the segment
                tail.front_mut().0 = to.0;
                tail.front_mut().1 = to.1;
                if dist == pos_distance_to_move {
                    // we advanced by the correct amount
                    break;
                } else {
                    // add a new point
                    pos_distance_to_move -= dist;
                    tail.0.push_front(to.clone());
                }
            } else {
                trace!("finished moving head point on the tail path");
                // we found the segment on which the head point is
                tail.front_mut().0 += from.1.delta() * pos_distance_to_move;
                tail.front_mut().1 = from.1;
                break;
            }
        }

        // 3. then shorten the back of the tail
        // NOTE: we only shorten according to the current length of the tail, we don't apply interpolation
        //  on the length.target here... since this function is only used for visual interpolation
        tail.shorten_by(pos_distance_to_move);
        tail
    }
}


#[derive(Component, Message, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
pub struct Speed(pub f32);


#[derive(Component, Message, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
pub struct Acceleration(pub f32);


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tail_pairs() {
        let tail = TailPoints(VecDeque::from(vec![
            (Vec2::new(0.0, 0.0), Direction::Right),
            (Vec2::new(0.0, 1.0), Direction::Down),
            (Vec2::new(-2.0, 1.0), Direction::Right),
        ]));
        assert_eq!(tail.pairs_front_to_back().collect_vec(), vec![
            (&(Vec2::new(0.0, 1.0), Direction::Down), &(Vec2::new(0.0, 0.0), Direction::Right)),
            (&(Vec2::new(-2.0, 1.0), Direction::Right), &(Vec2::new(0.0, 1.0), Direction::Down)),
        ]);
        assert_eq!(tail.pairs_back_to_front().collect_vec(), vec![
            (&(Vec2::new(-2.0, 1.0), Direction::Right), &(Vec2::new(0.0, 1.0), Direction::Down)),
            (&(Vec2::new(0.0, 1.0), Direction::Down), &(Vec2::new(0.0, 0.0), Direction::Right)),
        ]);


    }
}