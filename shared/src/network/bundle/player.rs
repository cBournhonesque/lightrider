use bevy::prelude::*;
use lightyear::prelude::{NetworkTarget, PeerId, Replicate};

use crate::network::protocol::prelude::{
    Player, PlayerRank, PlayerScore, PlayerStats, PlayerStatus, RoomId,
};

#[derive(Bundle)]
pub struct PlayerBundle {
    pub player: Player,
    pub score: PlayerScore,
    pub stats: PlayerStats,
    pub rank: PlayerRank,
    pub status: PlayerStatus,
    pub room: RoomId,
}

impl PlayerBundle {
    pub fn new(player: Player) -> Self {
        Self::new_in_room(player, RoomId::default())
    }

    pub fn new_in_room(player: Player, room: RoomId) -> Self {
        Self {
            player,
            score: PlayerScore::default(),
            stats: PlayerStats::default(),
            rank: PlayerRank::default(),
            status: PlayerStatus::Alive,
            room,
        }
    }

    pub fn spawn(self, commands: &mut Commands, _client_id: PeerId) -> Entity {
        commands
            .spawn((self, Replicate::to_clients(NetworkTarget::All)))
            .id()
    }
}
