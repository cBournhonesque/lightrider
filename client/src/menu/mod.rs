//! Main menu before we connect to the game.
//! The player can enter their name and choose a server to connect to.
//! When they press enter, we will get a ConnectToken from the backend,
//! which we will use to connect to the server.

use bevy::prelude::*;
use lightyear::connection::netcode::ConnectToken;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        // NotConnected
        // app.add_systems(Update, start_connection.run_if(in_state(AppState::NotConnected)));

        // app.add_systems(OnEnter(AppState::Connecting), try_to_connect);
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States, Reflect)]
enum AppState {
    #[default]
    NotConnected,
    Connecting,
    Connected,
}

pub fn start_connection(keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::Enter) {
        // move to Connecting?
        // next state = Connecting
    }
}


/// Issue a request to the backend to get a connect token
pub fn get_connect_token() -> ConnectToken {
    todo!()
}

/// Get the ConnectToken from a resource, and use it
pub fn connect(world: &mut World) {
    // TODO: can we do a blocking io op here? or will it block all other systems?
    // lightyear::client::resource::connect_with_token(world, )

    // if connection worked:
    // - next state = Connected
    // else:
    // - next state = Disconnected
}