use std::collections::VecDeque;
use std::vec::Vec;

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::math::curve::{Curve, Ease, FunctionCurve, Interval};
use bevy::prelude::*;
use bevy_replicon::prelude::Diffable as RepliconDiffable;
use derive_more::{Add, Mul};
use itertools::Itertools;
use lightyear::prelude::{Diffable as LightyearDiffable, Tick};
use parry2d::math::Point;
use serde::{Deserialize, Serialize};

const TAIL_POINT_ROLLBACK_EPSILON: f32 = 0.5;
const TAIL_LENGTH_ROLLBACK_EPSILON: f32 = 0.5;
const TAIL_VISUAL_CORRECTION_EPSILON: f32 = 0.05;
const SPEED_ROLLBACK_EPSILON: f32 = 0.02;
const ACCELERATION_ROLLBACK_EPSILON: f32 = 0.02;
const FOOD_BOOST_ROLLBACK_EPSILON: f32 = 0.02;

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

#[derive(
    Component, Deserialize, Serialize, Clone, Debug, Default, PartialEq, Reflect, Add, Mul,
)]
pub struct TailLength {
    pub current_size: f32,
    pub target_size: f32,
}

#[derive(Component, Clone, Debug, Default)]
pub struct TailPathHistory {
    pub head_distance: f32,
    samples: VecDeque<TailPathSample>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TailPathSample {
    pub tick: Tick,
    pub head_distance: f32,
    pub length: f32,
}

#[derive(Component, Deserialize, Serialize, Clone, Debug, Reflect)]
// tail inflection points, from front (head point) to back (tail end point)
pub struct TailPoints(pub VecDeque<(Vec2, Direction)>);

impl TailPoints {
    pub fn new(points: VecDeque<(Vec2, Direction)>) -> Self {
        Self(points)
    }
}

impl TailPathHistory {
    pub fn advance_head(&mut self, distance: f32) {
        self.head_distance += distance.max(0.0);
    }

    pub fn record_sample(&mut self, tick: Tick, length: f32, max_samples: usize) {
        let sample = TailPathSample {
            tick,
            head_distance: self.head_distance,
            length: length.max(0.0),
        };
        if let Some(last) = self.samples.back_mut().filter(|last| last.tick == tick) {
            *last = sample;
        } else {
            self.samples.push_back(sample);
        }

        let max_samples = max_samples.max(1);
        let excess = self.samples.len().saturating_sub(max_samples);
        if excess > 0 {
            self.samples.drain(..excess);
        }
    }

    pub fn sample_at(&self, tick: Tick, overstep: f32) -> Option<TailPathSample> {
        let first = *self.samples.front()?;
        if tick <= first.tick {
            return Some(first);
        }

        let last = *self.samples.back()?;
        if tick >= last.tick {
            return Some(last);
        }

        let start_index = self.samples.iter().position(|sample| sample.tick >= tick)?;
        let start = self.samples.get(start_index).copied()?;
        if start.tick != tick || overstep <= f32::EPSILON {
            return Some(start);
        }

        let end = self.samples.get(start_index + 1).copied().unwrap_or(start);
        let t = overstep.clamp(0.0, 1.0);
        Some(TailPathSample {
            tick: start.tick,
            head_distance: start.head_distance + (end.head_distance - start.head_distance) * t,
            length: start.length + (end.length - start.length) * t,
        })
    }
}

impl PartialEq for TailPoints {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub enum TailPointsOp {
    SetFrontDirectionAndPush {
        position: Vec2,
        direction: Direction,
    },
    SetFrontPosition(Vec2),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TailPointsCorrection {
    offsets: Vec<Vec2>,
}

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

    pub fn clipped_to_length(&self, length: f32) -> Self {
        let mut clipped = self.clone();
        let excess = clipped.total_length() - length.max(0.0);
        if excess > 0.0 {
            clipped.shorten_by(excess);
        }
        clipped
    }

    /// Shorten the tail by a certain amount
    pub fn shorten_by(&mut self, mut shorten_amount: f32) {
        if shorten_amount <= 0.0 || self.0.len() < 2 {
            return;
        }

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

    pub fn set_front_position(&mut self, position: Vec2) {
        self.front_mut().0 = position;
    }

    pub fn set_front_direction_and_push(&mut self, direction: Direction) {
        self.front_mut().1 = direction;
        let head = *self.front();
        self.0.push_front(head);
    }
}

impl RepliconDiffable for TailPoints {
    type Patch = TailPointsOp;

    const HISTORY_LEN: usize = 512;

    fn apply_patch(&mut self, patch: &Self::Patch) -> Result<()> {
        match *patch {
            TailPointsOp::SetFrontDirectionAndPush {
                position,
                direction,
            } => {
                self.set_front_position(position);
                self.set_front_direction_and_push(direction);
            }
            TailPointsOp::SetFrontPosition(position) => {
                self.set_front_position(position);
            }
        }
        Ok(())
    }
}

pub fn interpolate_tail_length(start: TailLength, end: TailLength, t: f32) -> TailLength {
    let t = t.clamp(0.0, 1.0);
    TailLength {
        current_size: start.current_size + (end.current_size - start.current_size) * t,
        target_size: start.target_size + (end.target_size - start.target_size) * t,
    }
}

pub fn interpolate_tail_length_correction(
    start: TailLength,
    end: TailLength,
    t: f32,
) -> TailLength {
    let interpolated = interpolate_tail_length(start, end, t);
    if interpolated
        .current_size
        .abs()
        .max(interpolated.target_size.abs())
        <= TAIL_VISUAL_CORRECTION_EPSILON
    {
        TailLength::default()
    } else {
        interpolated
    }
}

impl Ease for TailLength {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            interpolate_tail_length(start.clone(), end.clone(), t)
        })
    }
}

impl LightyearDiffable for TailLength {
    fn base_value() -> Self {
        Self::default()
    }

    fn diff(&self, new: &Self) -> Self {
        Self {
            current_size: new.current_size - self.current_size,
            target_size: new.target_size - self.target_size,
        }
    }

    fn apply_diff(&mut self, delta: &Self) {
        self.current_size += delta.current_size;
        self.target_size += delta.target_size;
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

pub fn interpolate_tail_points_correction(
    start: TailPointsCorrection,
    end: TailPointsCorrection,
    t: f32,
) -> TailPointsCorrection {
    let t = t.clamp(0.0, 1.0);
    let len = start.offsets.len().max(end.offsets.len());
    let offsets = (0..len)
        .map(|index| {
            start
                .offsets
                .get(index)
                .copied()
                .unwrap_or(Vec2::ZERO)
                .lerp(end.offsets.get(index).copied().unwrap_or(Vec2::ZERO), t)
        })
        .collect::<Vec<_>>();

    let max_offset = offsets
        .iter()
        .map(|offset| offset.length())
        .fold(0.0, f32::max);
    if max_offset <= TAIL_VISUAL_CORRECTION_EPSILON {
        TailPointsCorrection::default()
    } else {
        TailPointsCorrection { offsets }
    }
}

impl Ease for TailPointsCorrection {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            interpolate_tail_points_correction(start.clone(), end.clone(), t)
        })
    }
}

impl LightyearDiffable<TailPointsCorrection> for TailPoints {
    fn base_value() -> Self {
        TailPoints::new(VecDeque::new())
    }

    fn diff(&self, new: &Self) -> TailPointsCorrection {
        if self.0.len() != new.0.len() {
            return TailPointsCorrection::default();
        }

        let mut offsets = Vec::with_capacity(self.0.len());
        for ((current_point, current_direction), (visual_point, visual_direction)) in
            self.0.iter().zip(new.0.iter())
        {
            if current_direction != visual_direction {
                return TailPointsCorrection::default();
            }
            offsets.push(*visual_point - *current_point);
        }
        TailPointsCorrection { offsets }
    }

    fn apply_diff(&mut self, delta: &TailPointsCorrection) {
        if delta.offsets.is_empty() {
            return;
        }
        if self.0.is_empty() {
            self.0 = delta
                .offsets
                .iter()
                .map(|offset| (*offset, Direction::Right))
                .collect();
            return;
        }
        if self.0.len() != delta.offsets.len() {
            return;
        }
        for ((point, _), offset) in self.0.iter_mut().zip(delta.offsets.iter()) {
            *point += *offset;
        }
    }
}

pub fn tail_points_should_rollback(confirmed: &TailPoints, predicted: &TailPoints) -> bool {
    if confirmed.0.len() != predicted.0.len() {
        return true;
    }
    confirmed.0.iter().zip(predicted.0.iter()).any(
        |((confirmed_point, confirmed_direction), (predicted_point, predicted_direction))| {
            confirmed_direction != predicted_direction
                || confirmed_point.distance(*predicted_point) > TAIL_POINT_ROLLBACK_EPSILON
        },
    )
}

pub fn tail_length_should_rollback(confirmed: &TailLength, predicted: &TailLength) -> bool {
    (confirmed.current_size - predicted.current_size).abs() > TAIL_LENGTH_ROLLBACK_EPSILON
        || (confirmed.target_size - predicted.target_size).abs() > TAIL_LENGTH_ROLLBACK_EPSILON
}

pub fn speed_should_rollback(confirmed: &Speed, predicted: &Speed) -> bool {
    (confirmed.0 - predicted.0).abs() > SPEED_ROLLBACK_EPSILON
}

pub fn acceleration_should_rollback(confirmed: &Acceleration, predicted: &Acceleration) -> bool {
    (confirmed.0 - predicted.0).abs() > ACCELERATION_ROLLBACK_EPSILON
}

pub fn food_boost_should_rollback(confirmed: &FoodBoost, predicted: &FoodBoost) -> bool {
    (confirmed.0 - predicted.0).abs() > FOOD_BOOST_ROLLBACK_EPSILON
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
            if tail.front().1 != from.1 {
                tail.front_mut().1 = from.1;
                tail.0.push_front((start_head, from.1));
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

#[derive(
    Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Reflect, Add, Mul,
)]
pub struct FoodBoost(pub f32);

#[cfg(test)]
mod tests {
    use super::*;

    fn tail_is_axis_aligned(tail: &TailPoints) -> bool {
        tail.pairs_front_to_back()
            .all(|(start, end)| start.0.x == end.0.x || start.0.y == end.0.y)
    }

    #[test]
    fn test_tail_pairs() {
        let tail = TailPoints::new(VecDeque::from(vec![
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
    fn turn_patch_uses_authoritative_corner_position() {
        let mut tail = TailPoints::new(VecDeque::from([
            (Vec2::ZERO, Direction::Right),
            (Vec2::new(-100.0, 0.0), Direction::Right),
        ]));

        RepliconDiffable::apply_patch(
            &mut tail,
            &TailPointsOp::SetFrontDirectionAndPush {
                position: Vec2::new(25.0, 0.0),
                direction: Direction::Up,
            },
        )
        .unwrap();
        RepliconDiffable::apply_patch(
            &mut tail,
            &TailPointsOp::SetFrontPosition(Vec2::new(25.0, 10.0)),
        )
        .unwrap();

        assert_eq!(
            tail.0,
            VecDeque::from([
                (Vec2::new(25.0, 10.0), Direction::Up),
                (Vec2::new(25.0, 0.0), Direction::Up),
                (Vec2::new(-100.0, 0.0), Direction::Right),
            ])
        );
    }

    #[test]
    fn interpolate_inserts_corner_when_start_head_is_inside_new_direction_segment() {
        let start = TailPoints::new(VecDeque::from([
            (Vec2::new(10.0, 0.0), Direction::Up),
            (Vec2::new(10.0, -100.0), Direction::Up),
        ]));
        let end = TailPoints::new(VecDeque::from([
            (Vec2::new(50.0, 0.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Right),
            (Vec2::new(0.0, -50.0), Direction::Up),
        ]));
        let length = TailLength {
            current_size: 100.0,
            target_size: 100.0,
        };

        let (tail, _) = interpolate_tail_points_with_length(&start, &end, &length, &length, 0.5);

        assert!(tail_is_axis_aligned(&tail));
        assert_eq!(
            tail.0,
            VecDeque::from([
                (Vec2::new(30.0, 0.0), Direction::Right),
                (Vec2::new(10.0, 0.0), Direction::Right),
                (Vec2::new(10.0, -80.0), Direction::Up),
            ])
        );
    }

    #[test]
    fn interpolate_single_segment() {
        let start = TailPoints::new(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let end = TailPoints::new(VecDeque::from([
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
        let start = TailPoints::new(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -120.0), Direction::Up),
        ]));
        let end = TailPoints::new(VecDeque::from([
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
        let start = TailPoints::new(VecDeque::from([
            (Vec2::new(40.0, 50.0), Direction::Right),
            (Vec2::new(0.0, 50.0), Direction::Right),
            (Vec2::new(0.0, -10.0), Direction::Up),
        ]));
        let end = TailPoints::new(VecDeque::from([
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
        let start = TailPoints::new(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let end = TailPoints::new(VecDeque::from([
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
        let start = TailPoints::new(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let end = TailPoints::new(VecDeque::from([
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

    #[test]
    fn tail_length_correction_decays_towards_zero() {
        let error = TailLength {
            current_size: 4.0,
            target_size: -2.0,
        };

        assert_eq!(
            interpolate_tail_length_correction(TailLength::default(), error.clone(), 0.25),
            TailLength {
                current_size: 1.0,
                target_size: -0.5,
            }
        );
        assert_eq!(
            interpolate_tail_length_correction(TailLength::default(), error, 0.001),
            TailLength::default()
        );
    }

    #[test]
    fn tail_points_correction_applies_and_decays_offsets() {
        let corrected = TailPoints::new(VecDeque::from([
            (Vec2::new(10.0, 20.0), Direction::Right),
            (Vec2::new(0.0, 20.0), Direction::Right),
        ]));
        let visual = TailPoints::new(VecDeque::from([
            (Vec2::new(12.0, 20.0), Direction::Right),
            (Vec2::new(2.0, 20.0), Direction::Right),
        ]));
        let error = corrected.diff(&visual);
        let residual =
            interpolate_tail_points_correction(TailPointsCorrection::default(), error, 0.5);
        let mut smoothed = corrected.clone();

        smoothed.apply_diff(&residual);

        assert_eq!(
            smoothed.0,
            VecDeque::from([
                (Vec2::new(11.0, 20.0), Direction::Right),
                (Vec2::new(1.0, 20.0), Direction::Right),
            ])
        );
        assert_eq!(
            interpolate_tail_points_correction(TailPointsCorrection::default(), residual, 0.001,),
            TailPointsCorrection::default()
        );
    }

    #[test]
    fn rollback_checks_tolerate_tiny_snake_float_drift() {
        let confirmed_tail = TailPoints::new(VecDeque::from([
            (Vec2::new(10.0, 20.0), Direction::Right),
            (Vec2::new(0.0, 20.0), Direction::Right),
        ]));
        let close_tail = TailPoints::new(VecDeque::from([
            (Vec2::new(10.2, 20.0), Direction::Right),
            (Vec2::new(0.2, 20.0), Direction::Right),
        ]));
        let far_tail = TailPoints::new(VecDeque::from([
            (Vec2::new(10.75, 20.0), Direction::Right),
            (Vec2::new(0.2, 20.0), Direction::Right),
        ]));
        let wrong_direction = TailPoints::new(VecDeque::from([
            (Vec2::new(10.2, 20.0), Direction::Up),
            (Vec2::new(0.2, 20.0), Direction::Right),
        ]));

        assert!(!tail_points_should_rollback(&confirmed_tail, &close_tail));
        assert!(tail_points_should_rollback(&confirmed_tail, &far_tail));
        assert!(tail_points_should_rollback(
            &confirmed_tail,
            &wrong_direction
        ));
        assert!(!speed_should_rollback(&Speed(1.0), &Speed(1.01)));
        assert!(speed_should_rollback(&Speed(1.0), &Speed(1.05)));
        assert!(!acceleration_should_rollback(
            &Acceleration(0.04),
            &Acceleration(0.055)
        ));
        assert!(acceleration_should_rollback(
            &Acceleration(0.04),
            &Acceleration(0.08)
        ));
        assert!(!food_boost_should_rollback(
            &FoodBoost(0.02),
            &FoodBoost(0.03)
        ));
        assert!(food_boost_should_rollback(
            &FoodBoost(0.02),
            &FoodBoost(0.06)
        ));
    }
}
