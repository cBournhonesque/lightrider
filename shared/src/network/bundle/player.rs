use bevy::prelude::*;
use lightyear::prelude::{NetworkTarget, PeerId, Replicate, ReplicationGroup};

use crate::network::protocol::prelude::{
    Player, PlayerInput, PlayerRank, PlayerScore, PlayerStatus, RoomId,
};

#[derive(Bundle)]
pub struct PlayerBundle {
    pub player: Player,
    pub score: PlayerScore,
    pub rank: PlayerRank,
    pub status: PlayerStatus,
    pub input: PlayerInput,
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
            rank: PlayerRank::default(),
            status: PlayerStatus::Alive,
            input: PlayerInput,
            room,
        }
    }

    pub fn spawn(self, commands: &mut Commands, _client_id: PeerId) -> Entity {
        commands
            .spawn((
                self,
                Replicate::to_clients(NetworkTarget::None),
                ReplicationGroup::new_from_entity(),
            ))
            .id()
    }
}
