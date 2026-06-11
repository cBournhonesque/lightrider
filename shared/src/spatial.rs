use std::cmp::Ordering;
use std::collections::HashMap;

use bevy::prelude::*;

use crate::network::protocol::prelude::{RoomId, TailPoints};

const AXIS_EPSILON: f32 = 0.001;
const DEFAULT_TAIL_CELL_SIZE: f32 = 64.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentOrientation {
    Horizontal,
    Vertical,
    Diagonal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TailSegment {
    pub owner: Entity,
    pub room: RoomId,
    pub index: usize,
    pub orientation: SegmentOrientation,
    pub start: Vec2,
    pub end: Vec2,
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl TailSegment {
    fn new(owner: Entity, room: RoomId, index: usize, start: Vec2, end: Vec2) -> Option<Self> {
        if start.distance_squared(end) <= AXIS_EPSILON * AXIS_EPSILON {
            return None;
        }
        let orientation = if (start.x - end.x).abs() <= AXIS_EPSILON {
            SegmentOrientation::Vertical
        } else if (start.y - end.y).abs() <= AXIS_EPSILON {
            SegmentOrientation::Horizontal
        } else {
            SegmentOrientation::Diagonal
        };
        Some(Self {
            owner,
            room,
            index,
            orientation,
            start,
            end,
            min_x: start.x.min(end.x),
            max_x: start.x.max(end.x),
            min_y: start.y.min(end.y),
            max_y: start.y.max(end.y),
        })
    }
}

#[derive(Clone, Debug)]
pub struct TailSpatialIndex {
    cell_size: f32,
    vertical: HashMap<(RoomId, i32), Vec<TailSegment>>,
    horizontal: HashMap<(RoomId, i32), Vec<TailSegment>>,
    diagonal: Vec<TailSegment>,
}

impl Default for TailSpatialIndex {
    fn default() -> Self {
        Self::new(DEFAULT_TAIL_CELL_SIZE)
    }
}

impl TailSpatialIndex {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size: cell_size.max(1.0),
            vertical: HashMap::new(),
            horizontal: HashMap::new(),
            diagonal: Vec::new(),
        }
    }

    pub fn from_tails<'a>(
        tails: impl IntoIterator<Item = (Entity, RoomId, &'a TailPoints)>,
    ) -> Self {
        let mut index = Self::default();
        for (owner, room, tail) in tails {
            index.insert_tail(owner, room, tail);
        }
        index
    }

    pub fn insert_tail(&mut self, owner: Entity, room: RoomId, tail: &TailPoints) {
        for (segment_index, (segment_start, segment_end)) in tail.pairs_front_to_back().enumerate()
        {
            let Some(segment) =
                TailSegment::new(owner, room, segment_index, segment_start.0, segment_end.0)
            else {
                continue;
            };
            match segment.orientation {
                SegmentOrientation::Vertical => {
                    let cell = self.cell(segment.start.x);
                    self.vertical.entry((room, cell)).or_default().push(segment);
                }
                SegmentOrientation::Horizontal => {
                    let cell = self.cell(segment.start.y);
                    self.horizontal
                        .entry((room, cell))
                        .or_default()
                        .push(segment);
                }
                SegmentOrientation::Diagonal => {
                    self.diagonal.push(segment);
                }
            }
        }
    }

    pub fn vertical_segments_near(
        &self,
        room: RoomId,
        min_x: f32,
        max_x: f32,
    ) -> Vec<&TailSegment> {
        let mut segments = self.axis_segments_near(&self.vertical, room, min_x, max_x);
        segments.extend(self.diagonal.iter().filter(|segment| {
            segment.room == room && ranges_overlap(segment.min_x, segment.max_x, min_x, max_x)
        }));
        segments
    }

    pub fn horizontal_segments_near(
        &self,
        room: RoomId,
        min_y: f32,
        max_y: f32,
    ) -> Vec<&TailSegment> {
        let mut segments = self.axis_segments_near(&self.horizontal, room, min_y, max_y);
        segments.extend(self.diagonal.iter().filter(|segment| {
            segment.room == room && ranges_overlap(segment.min_y, segment.max_y, min_y, max_y)
        }));
        segments
    }

    pub fn segment_count(&self) -> usize {
        self.vertical.values().map(Vec::len).sum::<usize>()
            + self.horizontal.values().map(Vec::len).sum::<usize>()
            + self.diagonal.len()
    }

    fn axis_segments_near<'a>(
        &self,
        buckets: &'a HashMap<(RoomId, i32), Vec<TailSegment>>,
        room: RoomId,
        min_axis: f32,
        max_axis: f32,
    ) -> Vec<&'a TailSegment> {
        let min_cell = self.cell(min_axis.min(max_axis));
        let max_cell = self.cell(min_axis.max(max_axis));
        let mut segments = Vec::new();
        for cell in min_cell..=max_cell {
            if let Some(bucket) = buckets.get(&(room, cell)) {
                segments.extend(bucket.iter());
            }
        }
        segments
    }

    fn cell(&self, value: f32) -> i32 {
        (value / self.cell_size).floor() as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodPoint {
    pub entity: Entity,
    pub room: RoomId,
    pub position: Vec2,
}

#[derive(Clone, Debug, Default)]
pub struct FoodSpatialIndex {
    rooms: HashMap<RoomId, FoodKdTree>,
}

impl FoodSpatialIndex {
    pub fn from_food(food: impl IntoIterator<Item = FoodPoint>) -> Self {
        let mut rooms: HashMap<RoomId, Vec<FoodPoint>> = HashMap::new();
        for point in food {
            rooms.entry(point.room).or_default().push(point);
        }
        Self {
            rooms: rooms
                .into_iter()
                .map(|(room, points)| (room, FoodKdTree::build(points)))
                .collect(),
        }
    }

    pub fn within_radius(&self, room: RoomId, center: Vec2, radius: f32) -> Vec<FoodPoint> {
        let Some(tree) = self.rooms.get(&room) else {
            return Vec::new();
        };
        let mut results = Vec::new();
        tree.within_radius(center, radius.max(0.0), &mut results);
        results.sort_by(|left, right| compare_food_by_distance_then_entity(center, left, right));
        results
    }
}

#[derive(Clone, Debug, Default)]
struct FoodKdTree {
    root: Option<Box<FoodKdNode>>,
}

impl FoodKdTree {
    fn build(mut points: Vec<FoodPoint>) -> Self {
        Self {
            root: build_food_node(&mut points, 0),
        }
    }

    fn within_radius(&self, center: Vec2, radius: f32, results: &mut Vec<FoodPoint>) {
        let radius_squared = radius * radius;
        if let Some(root) = &self.root {
            root.within_radius(center, radius_squared, results);
        }
    }
}

#[derive(Clone, Debug)]
struct FoodKdNode {
    point: FoodPoint,
    axis: usize,
    left: Option<Box<FoodKdNode>>,
    right: Option<Box<FoodKdNode>>,
}

impl FoodKdNode {
    fn within_radius(&self, center: Vec2, radius_squared: f32, results: &mut Vec<FoodPoint>) {
        if self.point.position.distance_squared(center) <= radius_squared {
            results.push(self.point);
        }

        let delta = axis_value(center, self.axis) - axis_value(self.point.position, self.axis);
        let delta_squared = delta * delta;
        let (near, far) = if delta <= 0.0 {
            (&self.left, &self.right)
        } else {
            (&self.right, &self.left)
        };
        if let Some(near) = near {
            near.within_radius(center, radius_squared, results);
        }
        if delta_squared <= radius_squared {
            if let Some(far) = far {
                far.within_radius(center, radius_squared, results);
            }
        }
    }
}

fn build_food_node(points: &mut [FoodPoint], depth: usize) -> Option<Box<FoodKdNode>> {
    if points.is_empty() {
        return None;
    }
    let axis = depth % 2;
    points.sort_by(|left, right| compare_food_by_axis(axis, left, right));
    let median = points.len() / 2;
    let (left, median_and_right) = points.split_at_mut(median);
    let (median_point, right) = median_and_right.split_first_mut().unwrap();
    Some(Box::new(FoodKdNode {
        point: *median_point,
        axis,
        left: build_food_node(left, depth + 1),
        right: build_food_node(right, depth + 1),
    }))
}

fn compare_food_by_axis(axis: usize, left: &FoodPoint, right: &FoodPoint) -> Ordering {
    axis_value(left.position, axis)
        .total_cmp(&axis_value(right.position, axis))
        .then_with(|| {
            axis_value(left.position, 1 - axis).total_cmp(&axis_value(right.position, 1 - axis))
        })
        .then_with(|| left.entity.to_bits().cmp(&right.entity.to_bits()))
}

fn compare_food_by_distance_then_entity(
    center: Vec2,
    left: &FoodPoint,
    right: &FoodPoint,
) -> Ordering {
    left.position
        .distance_squared(center)
        .total_cmp(&right.position.distance_squared(center))
        .then_with(|| left.entity.to_bits().cmp(&right.entity.to_bits()))
}

fn axis_value(position: Vec2, axis: usize) -> f32 {
    if axis == 0 {
        position.x
    } else {
        position.y
    }
}

fn ranges_overlap(left_min: f32, left_max: f32, right_min: f32, right_max: f32) -> bool {
    left_min <= right_max && right_min <= left_max
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::network::protocol::prelude::Direction;

    use super::*;

    fn tail(points: impl IntoIterator<Item = (Vec2, Direction)>) -> TailPoints {
        TailPoints::new(VecDeque::from_iter(points))
    }

    #[test]
    fn tail_index_separates_horizontal_and_vertical_segments() {
        let snake = Entity::from_bits(1);
        let room = RoomId(7);
        let tail = tail([
            (Vec2::new(10.0, 50.0), Direction::Up),
            (Vec2::new(10.0, 0.0), Direction::Up),
            (Vec2::new(-20.0, 0.0), Direction::Left),
        ]);
        let index = TailSpatialIndex::from_tails([(snake, room, &tail)]);

        let vertical = index.vertical_segments_near(room, 9.0, 11.0);
        assert_eq!(vertical.len(), 1);
        assert_eq!(vertical[0].orientation, SegmentOrientation::Vertical);
        assert_eq!(vertical[0].owner, snake);

        let horizontal = index.horizontal_segments_near(room, -1.0, 1.0);
        assert_eq!(horizontal.len(), 1);
        assert_eq!(horizontal[0].orientation, SegmentOrientation::Horizontal);
        assert_eq!(horizontal[0].owner, snake);

        assert!(index
            .vertical_segments_near(RoomId(8), 9.0, 11.0)
            .is_empty());
    }

    #[test]
    fn food_index_returns_near_food_in_deterministic_order() {
        let room = RoomId(1);
        let far = Entity::from_bits(1);
        let near_high_entity = Entity::from_bits(5);
        let near_low_entity = Entity::from_bits(3);
        let other_room = Entity::from_bits(2);
        let index = FoodSpatialIndex::from_food([
            FoodPoint {
                entity: far,
                room,
                position: Vec2::new(20.0, 0.0),
            },
            FoodPoint {
                entity: near_high_entity,
                room,
                position: Vec2::new(1.0, 0.0),
            },
            FoodPoint {
                entity: near_low_entity,
                room,
                position: Vec2::new(-1.0, 0.0),
            },
            FoodPoint {
                entity: other_room,
                room: RoomId(2),
                position: Vec2::ZERO,
            },
        ]);

        let found = index.within_radius(room, Vec2::ZERO, 2.0);
        assert_eq!(
            found.iter().map(|point| point.entity).collect::<Vec<_>>(),
            vec![near_low_entity, near_high_entity]
        );
    }
}
