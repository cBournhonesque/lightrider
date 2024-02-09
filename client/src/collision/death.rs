//! This module handles:
//! - the player's death
//! - switching from the game state to the dead state
//! - respawning logic
use bevy::app::{App, Plugin};
use bevy::prelude::*;
use leafwing_input_manager::plugin::ToggleActions;
use lightyear::client::events::MessageEvent;
use lightyear::client::prediction::Predicted;
use shared::network::protocol::{GameAction, PlayerMovement};
use shared::network::protocol::prelude::{HasPlayer, SnakeCollision};
use crate::network::inputs::Owned;

pub(crate) struct DeathPlugin;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States, Reflect)]
enum GameState {
    Dead,
    #[default]
    Alive,
}

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {

        // states
        app.add_state::<GameState>();

        // systems
        // TODO: toggling the actions is not enough, ideally we would disable/enable the entire input plugin
        // dead
        app.add_systems(OnEnter(GameState::Dead), enable_dead_actions);
        app.add_systems(Update, set_alive_state.run_if(in_state(GameState::Dead)));

        // alive
        app.add_systems(OnEnter(GameState::Alive), enable_alive_actions);

        // all
        app.add_systems(Update, handle_death_message);

        // reflect
        app.register_type::<GameState>();
    }
}

// 1. if it's our own death, enter death state
// 2. if it's someone else's death, play death animation
fn handle_death_message(
    mut next_state: ResMut<NextState<GameState>>,
    mut messages: EventReader<MessageEvent<SnakeCollision>>,
    player: Query<Entity, With<Owned>>,
) {
    for message in messages.read() {
        let message = message.message();
        trace!(?message, "Received death message");
        if message.killed == player.single() {
            debug!("I died");
            next_state.set(GameState::Dead);
        }
    }

}

// 1. press spawn, send message to server
// 2. server
fn enable_dead_actions(
    mut movement_toggle: ResMut<ToggleActions<PlayerMovement>>,
    mut action_toggle: ResMut<ToggleActions<GameAction>>,
) {
    trace!("Enable dead actions");
    movement_toggle.enabled = false;
    action_toggle.enabled = true;
}

fn enable_alive_actions(
    mut movement_toggle: ResMut<ToggleActions<PlayerMovement>>,
    mut action_toggle: ResMut<ToggleActions<GameAction>>,
) {
    trace!("Enable alive actions");
    movement_toggle.enabled = true;
    action_toggle.enabled = false;
}


/// When we receive a new predicted snake from the server, that means we respawn!
/// Switch the game state
fn set_alive_state(
    mut next_state: ResMut<NextState<GameState>>,
    my_snake: Query<Entity, (Added<HasPlayer>, With<Predicted>)>
) {
    if my_snake.iter().next().is_some() {
        trace!("Setting state to Alive");
        next_state.set(GameState::Alive);
    }
}