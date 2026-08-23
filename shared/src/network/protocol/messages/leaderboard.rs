use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::{Entity, Message};
use serde::{Deserialize, Serialize};

use crate::network::protocol::components::common::RoomId;
use crate::network::protocol::components::player::{PlayerStats, PlayerStatus};

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LeaderboardSnapshot {
    pub sequence: u32,
    pub room: RoomId,
    pub entries: Vec<LeaderboardEntrySnapshot>,
}

impl MapEntities for LeaderboardSnapshot {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        for entry in &mut self.entries {
            entry.map_entities(entity_mapper);
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LeaderboardEntrySnapshot {
    pub player: Entity,
    pub name: String,
    pub score: u32,
    pub rank: u16,
    pub status: PlayerStatus,
    pub stats: PlayerStats,
}

impl MapEntities for LeaderboardEntrySnapshot {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.player = entity_mapper.get_mapped(self.player);
    }
}
