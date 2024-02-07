use bevy::prelude::*;
use lightyear::prelude::NetworkTarget;
use lightyear::server::events::ConnectEvent;
use shared::network::protocol::prelude::*;
use crate::network::bundle::snake::SnakeBundle;

pub(crate) fn handle_connections(
    mut connections: EventReader<ConnectEvent>,
    mut commands: Commands,
) {
    for connection in connections.read() {
        let client_id = connection.context();
        SnakeBundle::spawn(&mut commands, *client_id);
    }
}