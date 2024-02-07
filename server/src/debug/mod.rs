use bevy::prelude::*;
use bevy_inspector_egui::quick::WorldInspectorPlugin;

pub(crate) mod snake;
mod camera;


pub(crate) struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(WorldInspectorPlugin::new());

        // debug: render things on server
        app.add_plugins(snake::SnakeRenderPlugin);
        app.add_plugins(camera::CameraPlugin);
    }
}