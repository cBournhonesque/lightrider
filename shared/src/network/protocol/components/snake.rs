use std::collections::VecDeque;

use bevy::prelude::*;
use derive_more::{Add, Mul};
use itertools::Itertools;
use lightyear::prelude::*;
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