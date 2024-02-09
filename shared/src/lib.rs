use bevy::app::{App, Plugin};

pub mod network;
pub mod movement;
pub mod utils;
pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            (movement::MovementPlugin, network::NetworkPlugin)
        );
    }
}