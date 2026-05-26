use std::collections::VecDeque;

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use derive_more::{Add, Mul};
use itertools::Itertools;
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

#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
pub struct TailLength {
    pub current_size: f32,
    pub target_size: f32,
}

#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
// tail inflection points, from front (head point) to back (tail end point)
pub struct TailPoints(pub VecDeque<(Vec2, Direction)>);

// TODO: replace this with Parent in bevy 0.13
#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
pub struct HasPlayer(pub Entity);

impl MapEntities for HasPlayer {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.0 = entity_mapper.get_mapped(self.0);
    }
}

impl TailPoints {
    pub fn front(&self) -> &(Vec2, Direction) {
        self.0.front().unwrap()
    }

    pub fn front_mut(&mut self) -> &mut (Vec2, Direction) {
        self.0.front_mut().unwrap()
    }
    pub fn pairs_front_to_back<'a>(
        &'a self,
    ) -> impl Iterator<Item = (&'a (Vec2, Direction), &'a (Vec2, Direction))> {
        self.0.iter().tuple_windows().map(|(a, b)| (b, a))
    }

    pub fn pairs_back_to_front<'a>(
        &'a self,
    ) -> impl Iterator<Item = (&'a (Vec2, Direction), &'a (Vec2, Direction))> {
        self.0.iter().rev().tuple_windows()
    }

    pub fn points_front_to_back(&self) -> Vec<Point<f32>> {
        self.0.iter().map(|(v, _)| Point::new(v.x, v.y)).collect()
    }

    pub fn total_length(&self) -> f32 {
        self.pairs_front_to_back()
            .map(|(from, to)| from.0.distance(to.0))
            .sum()
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

pub fn interpolate_tail_length(start: TailLength, end: TailLength, t: f32) -> TailLength {
    let t = t.clamp(0.0, 1.0);
    TailLength {
        current_size: start.current_size + (end.current_size - start.current_size) * t,
        target_size: start.target_size + (end.target_size - start.target_size) * t,
    }
}

pub fn interpolate_tail_points(start: TailPoints, end: TailPoints, t: f32) -> TailPoints {
    let start_length = TailLength {
        current_size: start.total_length(),
        target_size: start.total_length(),
    };
    let end_length = TailLength {
        current_size: end.total_length(),
        target_size: end.total_length(),
    };
    interpolate_tail_points_with_length(&start, &end, &start_length, &end_length, t).0
}

pub fn interpolate_tail_points_with_length(
    start_tail: &TailPoints,
    end_tail: &TailPoints,
    start_length: &TailLength,
    end_length: &TailLength,
    t: f32,
) -> (TailPoints, TailLength) {
    let t = t.clamp(0.0, 1.0);
    if t <= f32::EPSILON || start_tail.0.is_empty() || end_tail.0.is_empty() {
        return (
            start_tail.clone(),
            interpolate_tail_length(start_length.clone(), end_length.clone(), t),
        );
    }
    if (1.0 - t) <= f32::EPSILON {
        return (
            end_tail.clone(),
            interpolate_tail_length(start_length.clone(), end_length.clone(), t),
        );
    }

    let mut tail = start_tail.clone();
    let mut length = interpolate_tail_length(start_length.clone(), end_length.clone(), t);
    let start_head = tail.front().0;

    let mut tail_diff_length = 0.0;
    let mut pos_distance_to_move = 0.0;
    let mut segment_idx = None;

    for (i, (from, to)) in end_tail.pairs_front_to_back().enumerate() {
        if point_on_axis_aligned_segment(from.0, to.0, start_head) {
            tail_diff_length += to.0.distance(start_head);
            if start_head == from.0 && tail.front().1 != from.1 {
                tail.front_mut().1 = from.1;
                tail.0.push_front(from.clone());
            }
            pos_distance_to_move = t * tail_diff_length;
            segment_idx = Some(i);
            break;
        }
        tail_diff_length += from.0.distance(to.0);
    }

    let Some(segment_idx) = segment_idx else {
        return (end_tail.clone(), length);
    };
    if pos_distance_to_move <= f32::EPSILON {
        return (tail, length);
    }

    length.current_size += pos_distance_to_move;
    let skip_segments = end_tail.0.len().saturating_sub(2 + segment_idx);
    for (from, to) in end_tail.pairs_back_to_front().skip(skip_segments) {
        let dist = tail.front().0.distance(to.0);
        if dist <= pos_distance_to_move {
            tail.front_mut().0 = to.0;
            tail.front_mut().1 = to.1;
            if (dist - pos_distance_to_move).abs() <= f32::EPSILON {
                break;
            }
            pos_distance_to_move -= dist;
            tail.0.push_front(to.clone());
        } else {
            tail.front_mut().0 += from.1.delta() * pos_distance_to_move;
            tail.front_mut().1 = from.1;
            break;
        }
    }

    shorten_tail_to_length(&mut tail, &mut length);
    (tail, length)
}

fn shorten_tail_to_length(tail: &mut TailPoints, tail_length: &mut TailLength) {
    if tail_length.target_size >= tail_length.current_size {
        return;
    }

    let shorten_amount = tail_length.current_size - tail_length.target_size;
    tail.shorten_by(shorten_amount);
    tail_length.current_size = tail_length.target_size;
}

fn point_on_axis_aligned_segment(a: Vec2, b: Vec2, p: Vec2) -> bool {
    let cross = (b - a).perp_dot(p - a).abs();
    if cross > 1000.0 * f32::EPSILON {
        return false;
    }

    let min = a.min(b);
    let max = a.max(b);
    p.x >= min.x - f32::EPSILON
        && p.x <= max.x + f32::EPSILON
        && p.y >= min.y - f32::EPSILON
        && p.y <= max.y + f32::EPSILON
}

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
pub struct Speed(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
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
        assert_eq!(
            tail.pairs_front_to_back().collect_vec(),
            vec![
                (
                    &(Vec2::new(0.0, 1.0), Direction::Down),
                    &(Vec2::new(0.0, 0.0), Direction::Right)
                ),
                (
                    &(Vec2::new(-2.0, 1.0), Direction::Right),
                    &(Vec2::new(0.0, 1.0), Direction::Down)
                ),
            ]
        );
        assert_eq!(
            tail.pairs_back_to_front().collect_vec(),
            vec![
                (
                    &(Vec2::new(-2.0, 1.0), Direction::Right),
                    &(Vec2::new(0.0, 1.0), Direction::Down)
                ),
                (
                    &(Vec2::new(0.0, 1.0), Direction::Down),
                    &(Vec2::new(0.0, 0.0), Direction::Right)
                ),
            ]
        );
    }

    #[test]
    fn interpolate_single_segment() {
        let start = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let end = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 20.0), Direction::Up),
            (Vec2::new(0.0, -80.0), Direction::Up),
        ]));
        let length = TailLength {
            current_size: 100.0,
            target_size: 100.0,
        };

        let (tail, _) = interpolate_tail_points_with_length(&start, &end, &length, &length, 0.5);

        assert_eq!(
            tail.0,
            VecDeque::from([
                (Vec2::new(0.0, 10.0), Direction::Up),
                (Vec2::new(0.0, -90.0), Direction::Up)
            ])
        );
    }

    #[test]
    fn interpolate_turn_big_move() {
        let start = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -120.0), Direction::Up),
        ]));
        let end = TailPoints(VecDeque::from([
            (Vec2::new(50.0, 50.0), Direction::Right),
            (Vec2::new(0.0, 50.0), Direction::Right),
            (Vec2::new(0.0, -20.0), Direction::Up),
        ]));
        let length = TailLength {
            current_size: 120.0,
            target_size: 120.0,
        };

        let (tail, _) = interpolate_tail_points_with_length(&start, &end, &length, &length, 0.75);

        assert_eq!(
            tail.0,
            VecDeque::from([
                (Vec2::new(25.0, 50.0), Direction::Right),
                (Vec2::new(0.0, 50.0), Direction::Right),
                (Vec2::new(0.0, -45.0), Direction::Up)
            ])
        );
    }

    #[test]
    fn interpolate_turn_small_move() {
        let start = TailPoints(VecDeque::from([
            (Vec2::new(40.0, 50.0), Direction::Right),
            (Vec2::new(0.0, 50.0), Direction::Right),
            (Vec2::new(0.0, -10.0), Direction::Up),
        ]));
        let end = TailPoints(VecDeque::from([
            (Vec2::new(50.0, 50.0), Direction::Right),
            (Vec2::new(0.0, 50.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]));
        let length = TailLength {
            current_size: 100.0,
            target_size: 100.0,
        };

        let (tail, _) = interpolate_tail_points_with_length(&start, &end, &length, &length, 0.75);

        assert_eq!(
            tail.0,
            VecDeque::from([
                (Vec2::new(47.5, 50.0), Direction::Right),
                (Vec2::new(0.0, 50.0), Direction::Right),
                (Vec2::new(0.0, -2.5), Direction::Up)
            ])
        );
    }

    #[test]
    fn interpolate_on_turn_point() {
        let start = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let end = TailPoints(VecDeque::from([
            (Vec2::new(50.0, 50.0), Direction::Right),
            (Vec2::new(0.0, 50.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]));
        let length = TailLength {
            current_size: 100.0,
            target_size: 100.0,
        };

        let (tail, _) = interpolate_tail_points_with_length(&start, &end, &length, &length, 0.5);

        assert_eq!(
            tail.0,
            VecDeque::from([
                (Vec2::new(0.0, 50.0), Direction::Right),
                (Vec2::new(0.0, -50.0), Direction::Up)
            ])
        );
    }

    #[test]
    fn interpolate_immediate_turn() {
        let start = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let end = TailPoints(VecDeque::from([
            (Vec2::new(50.0, 50.0), Direction::Right),
            (Vec2::new(50.0, 0.0), Direction::Up),
            (Vec2::new(0.0, 0.0), Direction::Right),
        ]));
        let length = TailLength {
            current_size: 100.0,
            target_size: 100.0,
        };

        let (tail, _) = interpolate_tail_points_with_length(&start, &end, &length, &length, 0.5);

        assert_eq!(
            tail.0,
            VecDeque::from([
                (Vec2::new(50.0, 0.0), Direction::Up),
                (Vec2::new(0.0, 0.0), Direction::Right),
                (Vec2::new(0.0, -50.0), Direction::Up)
            ])
        );
    }

    #[test]
    fn interpolate_tail_length_lerps_both_fields() {
        let start = TailLength {
            current_size: 100.0,
            target_size: 120.0,
        };
        let end = TailLength {
            current_size: 200.0,
            target_size: 240.0,
        };

        assert_eq!(
            interpolate_tail_length(start, end, 0.25),
            TailLength {
                current_size: 125.0,
                target_size: 150.0,
            }
        );
    }
}
