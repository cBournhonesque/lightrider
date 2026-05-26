use bevy::prelude::*;
use bevy_turborand::RngComponent;

use crate::config::GameConfig;
use crate::network::protocol::prelude::RoomId;

pub struct MapPlugin;

pub const MAP_SIZE: f32 = 2000.0;

#[derive(Component)]
pub struct MapMarker;

#[derive(Component)]
pub struct MapSize {
    pub width: f32,
    pub height: f32,
}

impl MapPlugin {
    pub fn spawn_map(mut commands: Commands, config: Res<GameConfig>) {
        spawn_room_map(&mut commands, &config, RoomId::default());
    }
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, MapPlugin::spawn_map);
    }
}

pub fn spawn_room_map(commands: &mut Commands, config: &GameConfig, room: RoomId) -> Entity {
    commands
        .spawn((
            MapSize {
                width: config.arena.width,
                height: config.arena.height,
            },
            room,
            MapMarker,
            RngComponent::with_seed(room_seed(room)),
        ))
        .id()
}

fn room_seed(room: RoomId) -> u64 {
    room.0
        .wrapping_mul(6364136223846793005)
        .wrapping_add(0x9e3779b97f4a7c15)
}
