use bevy::prelude::*;
use lightyear::server::events::ConnectEvent;
use crate::network::bundle::snake::SnakeBundle;

pub struct DebugPlugin;


impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init);
        app.add_systems(Update, handle_connections);
    }
}

fn init(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

/// Server connection system, create a player upon connection
pub(crate) fn handle_connections(
    mut connections: EventReader<ConnectEvent>,
    mut commands: Commands,
) {
    for connection in connections.read() {
        SnakeBundle::spawn(&mut commands);
    }
}