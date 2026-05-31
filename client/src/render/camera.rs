use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_camera);
    }
}

fn init_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Tonemapping::TonyMcMapface,
        Bloom {
            intensity: 0.22,
            low_frequency_boost: 0.8,
            ..Bloom::NATURAL
        },
        DebandDither::Enabled,
    ));
}
