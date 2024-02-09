use bevy::prelude::*;
use lightyear::prelude::{ClientId, NetworkTarget, ReplicationGroup};

use shared::network::protocol::prelude::Player;
use shared::network::protocol::Replicate;

#[derive(Bundle)]
pub struct PlayerBundle {
    pub player: Player,
}

impl PlayerBundle {
    pub fn new(player: Player) -> Self {
        Self { player }
    }
    pub fn spawn(self, commands: &mut Commands, client_id: ClientId) -> Entity {
        let mut replicate = Replicate {
            prediction_target: NetworkTarget::Single(client_id),
            interpolation_target: NetworkTarget::AllExceptSingle(client_id),
            replication_group: ReplicationGroup::new_id(client_id),
            ..default()
        };
        commands.spawn((self, replicate)).id()
    }
}