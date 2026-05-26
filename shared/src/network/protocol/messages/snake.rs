use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::{Entity, Message};
use serde::{Deserialize, Serialize};

use crate::network::protocol::components::common::RoomId;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathReason {
    Collision,
    Boundary,
    Suicide,
}

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SnakeCollision {
    pub killer: Entity,
    pub killed: Entity,
    pub reason: DeathReason,
}

impl MapEntities for SnakeCollision {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.killer = entity_mapper.get_mapped(self.killer);
        self.killed = entity_mapper.get_mapped(self.killed);
    }
}

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerDeath {
    pub killer_player: Entity,
    pub killed_player: Entity,
    pub killer_snake: Entity,
    pub killed_snake: Entity,
    pub room: RoomId,
    pub reason: DeathReason,
}

impl MapEntities for PlayerDeath {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.killer_player = entity_mapper.get_mapped(self.killer_player);
        self.killed_player = entity_mapper.get_mapped(self.killed_player);
        self.killer_snake = entity_mapper.get_mapped(self.killer_snake);
        self.killed_snake = entity_mapper.get_mapped(self.killed_snake);
    }
}
