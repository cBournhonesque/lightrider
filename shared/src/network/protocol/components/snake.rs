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
const SNAKE_HEAD_ROLLBACK_EPSILON: f32 = 0.5;
const TAIL_LENGTH_ROLLBACK_EPSILON: f32 = 0.5;
const TAIL_VISUAL_CORRECTION_EPSILON: f32 = 0.05;
const TAIL_AXIS_REPAIR_EPSILON: f32 = 0.001;
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

    pub fn opposite(&self) -> Self {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
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

#[derive(Component, Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct SnakeHead {
    pub position: Vec2,
    pub direction: Direction,
}

impl Default for SnakeHead {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            direction: Direction::Up,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct TailTurn {
    pub position: Vec2,
    pub tailward_direction: Direction,
}

impl TailTurn {
    pub fn new(position: Vec2, tailward_direction: Direction) -> Self {
        Self {
            position,
            tailward_direction,
        }
    }
}

#[derive(Component, Deserialize, Serialize, Clone, Debug, Reflect)]
pub struct TailPoints {
    pub turns: VecDeque<TailTurn>,
    pub turn_path_length: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TailPolyline(pub VecDeque<(Vec2, Direction)>);

impl TailPoints {
    pub fn new(turns: VecDeque<TailTurn>) -> Self {
        let mut points = Self {
            turns,
            turn_path_length: 0.0,
        };
        points.recompute_turn_path_length();
        points
    }

    pub fn empty() -> Self {
        Self::new(VecDeque::new())
    }

    pub fn from_legacy_polyline(
        points: VecDeque<(Vec2, Direction)>,
    ) -> (SnakeHead, TailLength, Self) {
        let Some((position, direction)) = points.front().copied() else {
            return (SnakeHead::default(), TailLength::default(), Self::empty());
        };
        let head = SnakeHead {
            position,
            direction,
        };
        let polyline = TailPolyline(points);
        let length = TailLength {
            current_size: polyline.total_length(),
            target_size: polyline.total_length(),
        };
        let turns = polyline
            .0
            .iter()
            .enumerate()
            .skip(1)
            .take(polyline.0.len().saturating_sub(2))
            .map(|(index, (position, _))| {
                let tailward_direction = polyline
                    .0
                    .get(index + 1)
                    .map(|(_, next_headward_direction)| next_headward_direction.opposite())
                    .unwrap_or(direction.opposite());
                TailTurn::new(*position, tailward_direction)
            })
            .collect();
        (head, length, Self::new(turns))
    }

    pub fn push_turn(&mut self, turn: TailTurn) {
        self.turns.push_front(turn);
        self.recompute_turn_path_length();
    }

    pub fn remove_tail_turns(&mut self, count: usize) {
        for _ in 0..count {
            if self.turns.pop_back().is_none() {
                break;
            }
        }
        self.recompute_turn_path_length();
    }

    pub fn prune_to_length(&mut self, head: &SnakeHead, length: f32) -> usize {
        let retained_length = length.max(0.0);
        let mut distance = 0.0;
        let mut previous = head.position;
        let mut keep = self.turns.len();

        for (index, turn) in self.turns.iter().enumerate() {
            distance += previous.distance(turn.position);
            if distance >= retained_length - f32::EPSILON {
                keep = index;
                break;
            }
            previous = turn.position;
        }

        let removed = self.turns.len().saturating_sub(keep);
        if removed > 0 {
            self.turns.truncate(keep);
            self.recompute_turn_path_length();
        }
        removed
    }

    pub fn polyline(&self, head: &SnakeHead, length: f32) -> TailPolyline {
        let mut points = VecDeque::with_capacity(self.turns.len() + 2);
        points.push_back((head.position, head.direction));

        let mut remaining = length.max(0.0);
        let mut current = head.position;
        let mut tailward_direction = head.direction.opposite();
        for turn in &self.turns {
            if remaining <= f32::EPSILON {
                break;
            }

            let segment_length = current.distance(turn.position);
            if remaining <= segment_length + f32::EPSILON {
                let endpoint = current + tailward_direction.delta() * remaining.min(segment_length);
                if endpoint.distance_squared(current) > f32::EPSILON * f32::EPSILON {
                    points.push_back((endpoint, tailward_direction.opposite()));
                }
                return TailPolyline(points);
            }

            if segment_length > f32::EPSILON {
                points.push_back((turn.position, tailward_direction.opposite()));
            }
            remaining -= segment_length;
            current = turn.position;
            tailward_direction = turn.tailward_direction;
        }

        if remaining > f32::EPSILON {
            let endpoint = current + tailward_direction.delta() * remaining;
            if endpoint.distance_squared(current) > f32::EPSILON * f32::EPSILON {
                points.push_back((endpoint, tailward_direction.opposite()));
            }
        }

        TailPolyline(points)
    }

    pub fn recompute_turn_path_length(&mut self) {
        self.turn_path_length = self
            .turns
            .iter()
            .tuple_windows()
            .map(|(a, b)| a.position.distance(b.position))
            .sum();
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
        self.turns == other.turns
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub enum TailPointsDiff {
    PushTurn(TailTurn),
    RemoveTailTurns(u16),
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

impl TailPolyline {
    pub fn new(points: VecDeque<(Vec2, Direction)>) -> Self {
        Self(points)
    }

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

    pub fn axis_aligned(&self) -> Self {
        let Some(first) = self.0.front().copied() else {
            return self.clone();
        };

        let mut repaired = VecDeque::with_capacity(self.0.len());
        repaired.push_back(first);
        for point in self.0.iter().skip(1).copied() {
            let previous = *repaired
                .back()
                .expect("axis-aligned tail repair always keeps a front point");
            if tail_segment_is_axis_aligned(previous.0, point.0) {
                repaired.push_back(point);
                continue;
            }

            let corner = tail_axis_aligned_corner(previous.0, point.0, point.1);
            if previous.0.distance_squared(corner)
                > TAIL_AXIS_REPAIR_EPSILON * TAIL_AXIS_REPAIR_EPSILON
                && point.0.distance_squared(corner)
                    > TAIL_AXIS_REPAIR_EPSILON * TAIL_AXIS_REPAIR_EPSILON
            {
                repaired.push_back((
                    corner,
                    tail_direction_between(corner, previous.0).unwrap_or(previous.1),
                ));
            }
            repaired.push_back(point);
        }

        TailPolyline::new(repaired)
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

fn tail_segment_is_axis_aligned(start: Vec2, end: Vec2) -> bool {
    (start.x - end.x).abs() <= TAIL_AXIS_REPAIR_EPSILON
        || (start.y - end.y).abs() <= TAIL_AXIS_REPAIR_EPSILON
}

fn tail_axis_aligned_corner(front: Vec2, back: Vec2, back_direction: Direction) -> Vec2 {
    let delta = back_direction.delta();
    if delta.x.abs() >= delta.y.abs() {
        Vec2::new(front.x, back.y)
    } else {
        Vec2::new(back.x, front.y)
    }
}

fn tail_direction_between(start: Vec2, end: Vec2) -> Option<Direction> {
    let delta = end - start;
    if delta.x.abs() >= delta.y.abs() && delta.x.abs() > TAIL_AXIS_REPAIR_EPSILON {
        Some(if delta.x > 0.0 {
            Direction::Right
        } else {
            Direction::Left
        })
    } else if delta.y.abs() > TAIL_AXIS_REPAIR_EPSILON {
        Some(if delta.y > 0.0 {
            Direction::Up
        } else {
            Direction::Down
        })
    } else {
        None
    }
}

impl RepliconDiffable for TailPoints {
    type Diff = TailPointsDiff;

    const HISTORY_LEN: usize = 512;

    fn apply_diff(&mut self, diff: &Self::Diff) -> Result<()> {
        match *diff {
            TailPointsDiff::PushTurn(turn) => self.push_turn(turn),
            TailPointsDiff::RemoveTailTurns(count) => self.remove_tail_turns(usize::from(count)),
        }
        Ok(())
    }
}

pub fn interpolate_snake_head(start: SnakeHead, end: SnakeHead, t: f32) -> SnakeHead {
    let t = t.clamp(0.0, 1.0);
    SnakeHead {
        position: start.position.lerp(end.position, t),
        direction: if t < 0.5 {
            start.direction
        } else {
            end.direction
        },
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
    if t < 0.5 {
        start
    } else {
        end
    }
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
        TailPoints::empty()
    }

    fn diff(&self, new: &Self) -> TailPointsCorrection {
        if self.turns.len() != new.turns.len() {
            return TailPointsCorrection::default();
        }

        let mut offsets = Vec::with_capacity(self.turns.len());
        for (current_turn, visual_turn) in self.turns.iter().zip(new.turns.iter()) {
            if current_turn.tailward_direction != visual_turn.tailward_direction {
                return TailPointsCorrection::default();
            }
            offsets.push(visual_turn.position - current_turn.position);
        }
        TailPointsCorrection { offsets }
    }

    fn apply_diff(&mut self, delta: &TailPointsCorrection) {
        if delta.offsets.is_empty() {
            return;
        }
        if self.turns.is_empty() {
            self.turns = delta
                .offsets
                .iter()
                .map(|offset| TailTurn::new(*offset, Direction::Left))
                .collect();
            self.recompute_turn_path_length();
            return;
        }
        if self.turns.len() != delta.offsets.len() {
            return;
        }
        for (turn, offset) in self.turns.iter_mut().zip(delta.offsets.iter()) {
            turn.position += *offset;
        }
        self.recompute_turn_path_length();
    }
}

pub fn tail_points_should_rollback(confirmed: &TailPoints, predicted: &TailPoints) -> bool {
    if confirmed.turns.len() != predicted.turns.len() {
        return true;
    }
    confirmed
        .turns
        .iter()
        .zip(predicted.turns.iter())
        .any(|(confirmed_turn, predicted_turn)| {
            confirmed_turn.tailward_direction != predicted_turn.tailward_direction
                || confirmed_turn.position.distance(predicted_turn.position)
                    > TAIL_POINT_ROLLBACK_EPSILON
        })
}

pub fn tail_length_should_rollback(confirmed: &TailLength, predicted: &TailLength) -> bool {
    (confirmed.current_size - predicted.current_size).abs() > TAIL_LENGTH_ROLLBACK_EPSILON
        || (confirmed.target_size - predicted.target_size).abs() > TAIL_LENGTH_ROLLBACK_EPSILON
}

pub fn snake_head_should_rollback(confirmed: &SnakeHead, predicted: &SnakeHead) -> bool {
    confirmed.direction != predicted.direction
        || confirmed.position.distance(predicted.position) > SNAKE_HEAD_ROLLBACK_EPSILON
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

    fn tail_is_axis_aligned(tail: &TailPolyline) -> bool {
        tail.pairs_front_to_back()
            .all(|(start, end)| tail_segment_is_axis_aligned(start.0, end.0))
    }

    #[test]
    fn test_tail_pairs() {
        let tail = TailPolyline::new(VecDeque::from(vec![
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
    fn reconstructs_polyline_from_head_turns_and_length() {
        let head = SnakeHead {
            position: Vec2::new(50.0, 100.0),
            direction: Direction::Right,
        };
        let tail = TailPoints::new(VecDeque::from([
            TailTurn::new(Vec2::new(0.0, 100.0), Direction::Down),
            TailTurn::new(Vec2::new(0.0, 0.0), Direction::Left),
        ]));

        let polyline = tail.polyline(&head, 120.0);

        assert!(tail_is_axis_aligned(&polyline));
        assert_eq!(
            polyline.0,
            VecDeque::from([
                (Vec2::new(50.0, 100.0), Direction::Right),
                (Vec2::new(0.0, 100.0), Direction::Right),
                (Vec2::new(0.0, 30.0), Direction::Up),
            ])
        );
    }

    #[test]
    fn tail_polyline_axis_alignment_repairs_transient_diagonal_segments() {
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::new(10.0, 10.0), Direction::Right),
            (Vec2::ZERO, Direction::Right),
        ]));

        let repaired = tail.axis_aligned();

        assert!(tail_is_axis_aligned(&repaired));
        assert_eq!(
            repaired.0,
            VecDeque::from([
                (Vec2::new(10.0, 10.0), Direction::Right),
                (Vec2::new(10.0, 0.0), Direction::Up),
                (Vec2::ZERO, Direction::Right),
            ])
        );
    }

    #[test]
    fn pruning_removes_turns_past_tail_endpoint() {
        let head = SnakeHead {
            position: Vec2::new(50.0, 100.0),
            direction: Direction::Right,
        };
        let mut tail = TailPoints::new(VecDeque::from([
            TailTurn::new(Vec2::new(0.0, 100.0), Direction::Down),
            TailTurn::new(Vec2::new(0.0, 0.0), Direction::Left),
        ]));

        assert_eq!(tail.prune_to_length(&head, 120.0), 1);
        assert_eq!(
            tail.turns,
            VecDeque::from([TailTurn::new(Vec2::new(0.0, 100.0), Direction::Down)])
        );
    }

    #[test]
    fn turn_diffs_update_topology() {
        let mut tail = TailPoints::empty();

        RepliconDiffable::apply_diff(
            &mut tail,
            &TailPointsDiff::PushTurn(TailTurn::new(Vec2::new(25.0, 0.0), Direction::Left)),
        )
        .unwrap();
        RepliconDiffable::apply_diff(
            &mut tail,
            &TailPointsDiff::PushTurn(TailTurn::new(Vec2::new(25.0, 10.0), Direction::Down)),
        )
        .unwrap();
        RepliconDiffable::apply_diff(&mut tail, &TailPointsDiff::RemoveTailTurns(1)).unwrap();

        assert_eq!(
            tail.turns,
            VecDeque::from([TailTurn::new(Vec2::new(25.0, 10.0), Direction::Down)])
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
            TailTurn::new(Vec2::new(10.0, 20.0), Direction::Left),
            TailTurn::new(Vec2::new(0.0, 20.0), Direction::Down),
        ]));
        let visual = TailPoints::new(VecDeque::from([
            TailTurn::new(Vec2::new(12.0, 20.0), Direction::Left),
            TailTurn::new(Vec2::new(2.0, 20.0), Direction::Down),
        ]));
        let error = corrected.diff(&visual);
        let residual =
            interpolate_tail_points_correction(TailPointsCorrection::default(), error, 0.5);
        let mut smoothed = corrected.clone();

        LightyearDiffable::apply_diff(&mut smoothed, &residual);

        assert_eq!(
            smoothed.turns,
            VecDeque::from([
                TailTurn::new(Vec2::new(11.0, 20.0), Direction::Left),
                TailTurn::new(Vec2::new(1.0, 20.0), Direction::Down),
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
            TailTurn::new(Vec2::new(10.0, 20.0), Direction::Left),
            TailTurn::new(Vec2::new(0.0, 20.0), Direction::Down),
        ]));
        let close_tail = TailPoints::new(VecDeque::from([
            TailTurn::new(Vec2::new(10.2, 20.0), Direction::Left),
            TailTurn::new(Vec2::new(0.2, 20.0), Direction::Down),
        ]));
        let far_tail = TailPoints::new(VecDeque::from([
            TailTurn::new(Vec2::new(10.75, 20.0), Direction::Left),
            TailTurn::new(Vec2::new(0.2, 20.0), Direction::Down),
        ]));
        let wrong_direction = TailPoints::new(VecDeque::from([
            TailTurn::new(Vec2::new(10.2, 20.0), Direction::Up),
            TailTurn::new(Vec2::new(0.2, 20.0), Direction::Down),
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
