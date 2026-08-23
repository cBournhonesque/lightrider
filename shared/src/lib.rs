use bevy::app::{App, Plugin};

pub mod collision;
pub mod colors;
pub mod config;
pub mod debug;

pub mod map;
pub mod movement;
pub mod network;
pub mod spatial;
pub mod utils;

pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            config::ConfigPlugin,
            movement::MovementPlugin,
            network::NetworkPlugin,
            utils::rand::RandPlugin,
        ));
    }
}
