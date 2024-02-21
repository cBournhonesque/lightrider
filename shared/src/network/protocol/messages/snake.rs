use bevy::prelude::{Entity, EntityMapper, Event};
use lightyear::prelude::{LightyearMapEntities, Message};
use serde::{Deserialize, Serialize};

#[derive(Message, Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[message(custom_map)]
pub struct SnakeCollision {
    pub killer: Entity,
    pub killed: Entity,
}

impl LightyearMapEntities for SnakeCollision {
    // TODO: we cannot use map_entities(entity_mapper) twice! Need to rework the trait
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.killer = entity_mapper.map_entity(self.killer);
        self.killed = entity_mapper.map_entity(self.killed);
    }
}