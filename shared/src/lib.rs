use bevy::app::{App, Plugin};

pub mod bot;
pub mod collision;
pub mod config;

pub mod map;
pub mod movement;
pub mod network;
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
