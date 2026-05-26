//! This module handles:
//! - the player's death
//! - switching from the game state to the dead state
//! - respawning logic
use bevy::app::{App, Plugin};
use bevy::prelude::*;
use lightyear::prelude::{Client, Controlled, MessageReceiver, Predicted};
use shared::network::protocol::prelude::{HasPlayer, Player, PlayerDeath};

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
        app.init_state::<GameState>();

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
    mut receivers: Query<&mut MessageReceiver<PlayerDeath>, With<Client>>,
    player: Query<Entity, (With<Player>, With<Controlled>)>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let Ok(mut receiver) = receivers.single_mut() else {
        return;
    };
    for message in receiver.receive() {
        trace!(?message, "Received death message");
        if message.killed_player == player {
            debug!("I died");
            next_state.set(GameState::Dead);
        }
    }
}

// During dead state, show the death screen to the user
#[allow(dead_code)]
fn show_death_screen(_commands: Commands) {
    // commands.spawn(NodeBundle {
    //     style: Style {
    //         display: Display::Flex,
    //         position_type: PositionType::Absolute,
    //         flex_direction: FlexDirection::Column,
    //         justify_content: JustifyContent::Center,
    //         align_items: AlignItems::Center,
    //         ..Default::default()
    //     },
    //     ..Default::default()
    // })
    // })
}

// 1. press spawn, send message to server
// 2. server
fn enable_dead_actions() {
    trace!("Enable dead actions");
}

fn enable_alive_actions() {
    trace!("Enable alive actions");
}

// TODO: receive an event instead
/// When we receive a new predicted snake from the server, that means we respawn!
/// Switch the game state
fn set_alive_state(
    mut next_state: ResMut<NextState<GameState>>,
    my_snake: Query<Entity, (Added<HasPlayer>, With<Predicted>)>,
) {
    if my_snake.iter().next().is_some() {
        trace!("Setting state to Alive");
        next_state.set(GameState::Alive);
    }
}
