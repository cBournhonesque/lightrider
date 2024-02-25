use bevy::prelude::*;
use bevy_turborand::{GlobalRng, RngComponent};

pub struct MapPlugin;

pub const MAP_SIZE: f32 = 2000.0;

#[derive(Component)]
pub struct MapMarker;

#[derive(Component)]
pub struct MapSize{
    pub width: f32,
    pub height: f32
}

impl MapPlugin {

    pub fn spawn_map(mut commands: Commands,mut global_rng: ResMut<GlobalRng>) {
        commands.spawn((MapSize {
            width: MAP_SIZE,
            height: MAP_SIZE
        }, MapMarker, RngComponent::from(&mut global_rng)));
    }
}


impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, MapPlugin::spawn_map);
    }
}