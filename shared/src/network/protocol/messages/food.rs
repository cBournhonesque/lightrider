use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::{Entity, Message, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FoodCollision {
    pub snake: Entity,
    pub food: Entity,
    pub food_position: Vec2,
    pub head_position: Vec2,
}

impl MapEntities for FoodCollision {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.snake = entity_mapper.get_mapped(self.snake);
        self.food = entity_mapper.get_mapped(self.food);
    }
}
