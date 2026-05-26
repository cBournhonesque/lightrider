use bevy::app::{App, Plugin};
// use bevy_inspector_egui::quick::WorldInspectorPlugin;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, _app: &mut App) {
        // app.add_plugins(WorldInspectorPlugin::new());
    }
}
