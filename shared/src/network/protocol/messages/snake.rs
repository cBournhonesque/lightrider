use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::{Entity, Message, Reflect};
use serde::{Deserialize, Serialize};

use crate::network::protocol::components::common::RoomId;
use crate::network::protocol::components::player::PlayerStats;

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
    pub killer_name: String,
    pub killed_name: String,
    pub room: RoomId,
    pub reason: DeathReason,
    pub stats: PlayerDeathStats,
}

impl MapEntities for PlayerDeath {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.killer_player = entity_mapper.get_mapped(self.killer_player);
        self.killed_player = entity_mapper.get_mapped(self.killed_player);
        self.killer_snake = entity_mapper.get_mapped(self.killer_snake);
        self.killed_snake = entity_mapper.get_mapped(self.killed_snake);
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerDeathStats {
    pub average_speed: f32,
    pub score: u32,
    pub time_alive_seconds: f32,
    pub kills: u32,
    pub time_as_leader_seconds: f32,
    pub food_eaten: u32,
}

impl PlayerDeathStats {
    pub fn from_live(score: u32, stats: &PlayerStats) -> Self {
        Self {
            average_speed: stats.average_speed,
            score,
            time_alive_seconds: stats.time_alive_seconds,
            kills: stats.kills,
            time_as_leader_seconds: stats.time_as_leader_seconds,
            food_eaten: stats.food_eaten,
        }
    }
}
