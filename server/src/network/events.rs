use bevy::prelude::*;
use lightyear::server::events::ConnectEvent;
use crate::network::bundle::snake::SnakeBundle;

pub(crate) fn handle_connections(
    mut connections: EventReader<ConnectEvent>,
    mut commands: Commands,
) {
    for connection in connections.read() {
        info!("Spawning new snake");
        SnakeBundle::spawn(&mut commands);
    }
}