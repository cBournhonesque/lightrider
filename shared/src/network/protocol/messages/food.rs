use bevy::prelude::{Entity, EntityMapper, Event};
use lightyear::prelude::{LightyearMapEntities, Message};
use serde::{Deserialize, Serialize};

#[derive(Message, Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[message(custom_map)]
pub struct FoodCollision {
    pub snake: Entity,
    pub food: Entity,
}

impl LightyearMapEntities for FoodCollision {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.snake = entity_mapper.map_entity(self.snake);
        self.food = entity_mapper.map_entity(self.food);
    }
}