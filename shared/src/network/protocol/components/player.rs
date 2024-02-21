use bevy::prelude::{Component, Entity, EntityMapper, Reflect};
use lightyear::prelude::{ClientId, LightyearMapEntities, Message};
use serde::{Deserialize, Serialize};

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
#[message(custom_map)]
pub struct Player{
    pub id: ClientId,
    pub name: String,
    pub snake: Option<Entity>,
}

impl LightyearMapEntities for Player {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.snake.map(|x| entity_mapper.map_entity(x));
    }
}

