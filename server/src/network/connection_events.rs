use bevy::prelude::*;
use bevy::utils::HashMap;
use lightyear::prelude::ClientId;
use lightyear::server::events::{ConnectEvent, DisconnectEvent};

use shared::network::protocol::prelude::*;

use crate::network::bundle::player::PlayerBundle;
use crate::network::bundle::snake::SnakeBundle;

#[derive(Resource, Debug, Default)]
pub struct Global {
    // TODO: maybe lightyear can automatically create a Player entity, and maintain this map?
    /// map from client id to the player entity
    pub(crate) client_id_map: HashMap<ClientId, Entity>,
}

pub(crate) fn handle_connections(
    mut global: ResMut<Global>,
    mut connections: EventReader<ConnectEvent>,
    mut commands: Commands,
) {
    for connection in connections.read() {
        let client_id = connection.context();
        let head_entity = SnakeBundle::spawn(&mut commands, *client_id);
        let player_entity = PlayerBundle::new(Player {
            id: *client_id,
            name: "Player".to_string(),
            snake: Some(head_entity),
        }).spawn(&mut commands, *client_id);
        commands.entity(head_entity).insert(HasPlayer(player_entity));
        global.client_id_map.insert(*client_id, player_entity);
    }
}

pub(crate) fn handle_disconnections(
    mut global: ResMut<Global>,
    mut disconnects: EventReader<DisconnectEvent>,
    player_query: Query<&Player>,
    mut commands: Commands,
) {
    for disconnect in disconnects.read() {
        let client_id = disconnect.context();
        if let Some(player_entity) = global.client_id_map.remove(client_id) {
            if let Ok(player) = player_query.get(player_entity) {
                if let Some(snake_entity) = player.snake {
                    // TODO: to delete the tail, we need to maintain a child/parent relationship!
                    commands.entity(snake_entity).despawn_recursive();
                }
            }
            commands.entity(player_entity).despawn_recursive();
        }
    }
}