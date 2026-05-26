use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use clap::Parser;
use std::path::PathBuf;

use crate::food::FoodPlugin;
use shared::config::GameConfig;
use shared::SharedPlugin;

mod bots;
pub(crate) mod collision;
mod debug;
mod food;
mod network;
pub(crate) mod rooms;
mod spawning;

pub const SERVER_PORT: u16 = 5000;

#[derive(Parser, PartialEq, Debug)]
pub struct Cli {
    #[arg(long, default_value = "false")]
    headless: bool,

    #[arg(short, long, default_value = "false")]
    inspector: bool,

    #[arg(short, long, default_value_t = SERVER_PORT)]
    port: u16,

    #[arg(long)]
    config: Option<PathBuf>,
}

pub async fn app(cli: Cli) -> App {
    let mut app = App::new();
    app.insert_resource(load_config(cli.config.as_deref()));
    if cli.headless {
        app.add_plugins(MinimalPlugins);
        app.add_plugins(LogPlugin {
            level: Level::INFO,
            filter: "wgpu=error,bevy_ecs=trace".to_string(),
            ..Default::default()
        });
    } else {
        app.add_plugins(DefaultPlugins.set(LogPlugin {
            level: Level::INFO,
            filter: "wgpu=error,bevy_render=info,bevy_ecs=trace".to_string(),
            ..Default::default()
        }));
    }

    // shared
    app.add_plugins(SharedPlugin);

    // networking
    app.add_plugins(network::NetworkPluginGroup::new(cli.port).build());

    // rooms
    app.add_plugins(rooms::ServerRoomsPlugin);

    // debug
    app.add_plugins(debug::DebugPlugin);

    // collisions
    app.add_plugins(collision::CollisionPlugin);

    // bots
    app.add_plugins(bots::ServerBotsPlugin);

    // food
    app.add_plugins(FoodPlugin);
    app
}

fn load_config(path: Option<&std::path::Path>) -> GameConfig {
    let Some(path) = path else {
        return GameConfig::default();
    };
    #[cfg(not(target_family = "wasm"))]
    {
        GameConfig::from_ron_file(path)
            .unwrap_or_else(|error| panic!("failed to load config {}: {error:#}", path.display()))
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = path;
        GameConfig::default()
    }
}
