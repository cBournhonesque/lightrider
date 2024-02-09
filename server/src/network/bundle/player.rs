use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use lightyear::prelude::{ClientId, NetworkTarget, ReplicationGroup};

use shared::network::protocol::prelude::Player;
use shared::network::protocol::{GameAction, PlayerMovement, Replicate};

#[derive(Bundle)]
pub struct PlayerBundle {
    pub player: Player,
    // we need to include the action-state so that client inputs are replicated to the server
    pub action: ActionState<GameAction>,
}

impl PlayerBundle {
    pub fn new(player: Player) -> Self {
        Self { player, action: ActionState::default() }
    }
    pub fn spawn(self, commands: &mut Commands, client_id: ClientId) -> Entity {
        let mut replicate = Replicate {
            replication_group: ReplicationGroup::new_id(client_id),
            ..default()
        };
        // no need to replicate player inputs
        replicate.disable_component::<ActionState<GameAction>>();
        commands.spawn((self, replicate)).id()
    }
}