use std::collections::VecDeque;
use bevy::prelude::*;
use itertools::Itertools;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
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


#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct TailLength{
    pub current_size: f32,
    pub target_size: f32,
}

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct HeadPoint(pub Vec2);

#[derive(Component, Message, Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
pub struct HeadDirection(pub Direction);

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq)]
// tail inflection points, from front (point closest to the head) to back (tail end point)
pub struct TailPoints(pub VecDeque<(Vec2, Direction)>);

// TODO: replace this with Parent in bevy 0.13
#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq)]
#[message(custom_map)]
pub struct TailParent(pub Entity);

impl<'a> MapEntities<'a> for TailParent {
    fn map_entities(&mut self, entity_mapper: Box<dyn EntityMapper + 'a>) {
        self.0.map_entities(entity_mapper);
    }

    fn entities(&self) -> bevy::utils::EntityHashSet<Entity> {
        bevy::utils::EntityHashSet::from_iter(vec![self.0])
    }
}




impl TailPoints {
    pub fn pairs<'a>(&'a self, head: &'a (Vec2, Direction)) -> impl Iterator<Item = (&(Vec2, Direction), &(Vec2, Direction))> {
        std::iter::once(head).chain(self.0.iter()).tuple_windows().map(|(a, b)| (b, a))
    }
}


#[derive(Component, Message, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct Speed(pub f32);


#[derive(Component, Message, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct Acceleration(pub f32);


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tail_pairs() {
        let head = (Vec2::new(0.0, 0.0), Direction::Right);
        let tail = TailPoints(VecDeque::from(vec![
            (Vec2::new(0.0, 1.0), Direction::Down),
            (Vec2::new(-2.0, 1.0), Direction::Right),
        ]));
        assert_eq!(tail.pairs(&head).collect_vec(), vec![
            (&(Vec2::new(0.0, 1.0), Direction::Down), &(Vec2::new(0.0, 0.0), Direction::Right)),
            (&(Vec2::new(-2.0, 1.0), Direction::Right), &(Vec2::new(0.0, 1.0), Direction::Down)),
        ]);

    }
}