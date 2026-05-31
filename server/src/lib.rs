use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use clap::Parser;
use std::path::PathBuf;

use crate::food::FoodPlugin;
use shared::config::GameConfig;
use shared::debug::{runtime_log_plugin, RuntimeDebugPlugin};
use shared::SharedPlugin;

mod admin;
mod bots;
pub(crate) mod collision;
mod debug;
mod food;
#[cfg(feature = "lightyear-matchmaker")]
mod matchmaker;
mod network;
mod respawn;
pub(crate) mod rooms;
mod spawning;
mod stats;

pub const SERVER_PORT: u16 = 5000;

#[derive(Parser, PartialEq, Debug)]
pub struct Cli {
    #[arg(long, default_value = "false")]
    headless: bool,

    #[arg(short, long, default_value = "false")]
    inspector: bool,

    #[arg(short, long, default_value_t = SERVER_PORT)]
    port: u16,

    /// Enable Lightyear Matchmaker NATS integration.
    #[cfg(feature = "lightyear-matchmaker")]
    #[arg(long, alias = "bevygap", default_value = "false")]
    matchmaker: bool,

    #[arg(long)]
    config: Option<PathBuf>,
}

pub async fn app(cli: Cli) -> App {
    let mut app = App::new();
    let config = load_config(cli.config.as_deref());
    let log_plugin = if cli.headless {
        runtime_log_plugin(&config, "wgpu=error,bevy_ecs=trace")
    } else {
        runtime_log_plugin(&config, "wgpu=error,bevy_render=info,bevy_ecs=trace")
    };
    app.insert_resource(config.clone());
    if cli.headless {
        app.add_plugins(MinimalPlugins);
        app.add_plugins(StatesPlugin);
        app.add_plugins(log_plugin);
    } else {
        app.add_plugins(DefaultPlugins.set(log_plugin));
    }

    // networking
    #[cfg(feature = "lightyear-matchmaker")]
    let start_server_immediately = !cli.matchmaker;
    #[cfg(not(feature = "lightyear-matchmaker"))]
    let start_server_immediately = true;
    app.add_plugins(network::NetworkPluginGroup::new(cli.port, start_server_immediately).build());
    #[cfg(feature = "lightyear-matchmaker")]
    if cli.matchmaker {
        app.add_plugins(matchmaker::matchmaker_server_plugin(cli.port, &config));
    }

    // shared
    app.add_plugins(SharedPlugin);
    app.add_plugins(RuntimeDebugPlugin::server());

    // rooms
    app.add_plugins(rooms::ServerRoomsPlugin);
    #[cfg(feature = "lightyear-matchmaker")]
    if cli.matchmaker {
        app.add_plugins(matchmaker::LightriderMatchmakerMetricsPlugin);
    }

    // debug
    app.add_plugins(debug::DebugPlugin);

    // admin
    app.add_plugins(admin::ServerAdminPlugin);

    // collisions
    app.add_plugins(collision::CollisionPlugin);

    // bots
    app.add_plugins(bots::ServerBotsPlugin);

    // food
    app.add_plugins(FoodPlugin);

    // stats
    app.add_plugins(stats::ServerStatsPlugin);
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
