use bevy::app::{App, Plugin};
use bevy::prelude::{Commands, Entity, Query, Update};
use leafwing_input_manager::prelude::ActionState;
use lightyear::server::input_leafwing::LeafwingInputPlugin;
use tracing::info;

use shared::network::protocol::{DeadGameAction, GameProtocol, PlayerMovement};
use shared::network::protocol::prelude::{HasPlayer, Player};
use shared::network::bundle::snake::SnakeBundle;

pub struct NetworkInputsPlugin;


impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(LeafwingInputPlugin::<GameProtocol, PlayerMovement>::default());
        app.add_plugins(LeafwingInputPlugin::<GameProtocol, DeadGameAction>::default());

        // systems
        app.add_systems(Update, handle_game_action);
    }
}

fn handle_game_action(
    mut commands: Commands,
    mut players: Query<(Entity, &mut Player, &ActionState<DeadGameAction>)>
) {
    for (player_entity, mut player, action_state) in players.iter_mut() {
        if action_state.just_pressed(DeadGameAction::Spawn) {
            info!(?player, "Respawning player");
            let client_id = player.id;
            // respawn the snake
            let head_entity = SnakeBundle::spawn(&mut commands, client_id);
            commands.entity(head_entity).insert(HasPlayer(player_entity));
            player.snake = Some(head_entity);
        }
    }
}