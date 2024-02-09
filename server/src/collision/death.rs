use bevy::prelude::*;
use lightyear::prelude::NetworkTarget;
use tracing::error;
use shared::network::protocol::prelude::*;
use shared::network::protocol::ServerConnectionManager;
use crate::collision::collider::ColliderSet;

pub struct DeathPlugin;

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_collision.after(ColliderSet::ComputeCollision));
    }
}


pub fn handle_collision(
    mut reader: EventReader<SnakeCollision>,
    mut connection_manager: ResMut<ServerConnectionManager>,
    mut players: Query<&mut Player>,
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
        let Ok(mut killed) = players.get_mut(killed_player.0) else {
            error!("player could not be found");
            continue;
        };
        info!(?collision_event, "Collision event!");

        // we are sending this message so that the client can render the kill effects
        // TODO: send message to room instead!
        let _ = connection_manager.send_message_to_target::<GameChannel, _>(SnakeCollision {
            killer: killer_player.0,
            killed: killed_player.0,
        }, NetworkTarget::All).map_err(|e| error!(?e, "Failed to send message"));

        // despawn dead snake and remove snake from player
        commands.entity(collision_event.killed).despawn_recursive();
        killed.snake = None;
    }
}