use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::{Component, Entity, Reflect};
use lightyear::prelude::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
pub struct Player {
    pub id: PeerId,
    pub name: String,
    pub snake: Option<Entity>,
}

impl MapEntities for Player {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.snake = self.snake.map(|x| entity_mapper.get_mapped(x));
    }
}

#[derive(Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerScore {
    pub value: u32,
}

impl PlayerScore {
    pub fn from_length(length: f32) -> Self {
        Self {
            value: length.max(0.0).round() as u32,
        }
    }
}

#[derive(
    Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect,
)]
pub struct PlayerRank {
    /// One-based rank within the player's current room. `0` means unranked/not computed yet.
    pub value: u16,
}

#[derive(
    Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect,
)]
pub enum PlayerStatus {
    #[default]
    Alive,
    Dead,
}
