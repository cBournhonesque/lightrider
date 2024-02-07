use bevy::prelude::*;
use lightyear::server::events::ConnectEvent;

pub struct DebugPlugin;


impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init);
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
        let client_id = connection.context();
        // Generate pseudo random color from client id.
        let h = (((client_id.wrapping_mul(30)) % 360) as f32) / 360.0;
        let s = 0.8;
        let l = 0.5;
        let player_position = Vec2::ZERO;
        let player_entity = commands
            .spawn(PlayerBundle::new(
                *client_id,
                player_position,
                Color::hsl(h, s, l),
            ))
            .id();
        let tail_length = 300.0;
        let tail_entity = commands
            .spawn(TailBundle::new(
                *client_id,
                player_entity,
                player_position,
                tail_length,
            ))
            .id();
        // Add a mapping from client id to entity id
        global
            .client_id_to_entity_id
            .insert(*client_id, (player_entity, tail_entity));
    }
}