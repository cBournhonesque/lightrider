use bevy::prelude::*;
use lightyear::prelude::NetworkTarget;
use tracing::error;
use shared::network::protocol::prelude::*;
use shared::network::protocol::ServerConnectionManager;
use crate::collision::collider::SnakeCollisionEvent;

pub fn handle_collision(
    mut reader: EventReader<SnakeCollisionEvent>,
    mut connection_manager: ResMut<ServerConnectionManager>,
    players: Query<&Player>,
    snakes: Query<&HasPlayer>,
    mut commands: Commands,
) {
    for collision_event in reader.read() {
        let Ok(killed_player) = snakes.get(collision_event.killed) else {
            error!("snake does not have HasPlayer component");
            continue;
        };
        let Ok(killer_player) = snakes.get(collision_event.killer) else {
            error!("snake does not have HasPlayer component");
            continue;
        };
        let Ok(killed) = players.get(killed_player.0) else {
            error!("player could not be found");
            continue;
        };
        let Ok(killer) = players.get(killer_player.0) else {
            error!("player could not be found");
            continue;
        };
        // we are sending this message so that the client can render the kill effects
        // TODO: send message to room instead!
        connection_manager.send_message_to_target::<GameChannel, _>(SnakeCollision {
            killer: killer.id,
            killed: killed.id,
        }, NetworkTarget::All).map_err(|e| error!(?e, "Failed to send message"));

        // TODO: despawn snakes
        commands.entity(collision_event.killed).despawn_recursive();
    }
}