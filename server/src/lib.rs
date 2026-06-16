use bevy::app::AppExit;
use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use clap::Parser;
use std::{path::PathBuf, time::Duration};

use crate::food::FoodPlugin;
use shared::config::GameConfig;
use shared::debug::{runtime_log_plugin, RuntimeDebugPlugin};
use shared::SharedPlugin;

mod admin;
mod bots;
pub(crate) mod collision;
mod debug;
mod food;
mod interest;
mod leaderboard;
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

    /// Gracefully stop the server after this many seconds.
    ///
    /// This is mainly used by profiling recipes so trace writers can flush.
    #[arg(long)]
    run_seconds: Option<f64>,
}

#[derive(Resource)]
struct ExitAfterSeconds(Duration);

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
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 60.0,
            ))),
            StatesPlugin,
            log_plugin,
        ));
    } else {
        app.add_plugins(DefaultPlugins.set(log_plugin));
    }
    if let Some(seconds) = cli.run_seconds.filter(|seconds| *seconds > 0.0) {
        app.insert_resource(ExitAfterSeconds(Duration::from_secs_f64(seconds)));
        app.add_systems(Update, exit_after_run_seconds);
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
    app.add_plugins(interest::InterestPlugin);
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

    // leaderboard
    app.add_plugins(leaderboard::ServerLeaderboardPlugin);

    // stats
    app.add_plugins(stats::ServerStatsPlugin);
    app
}

fn exit_after_run_seconds(
    time: Res<Time<Real>>,
    limit: Res<ExitAfterSeconds>,
    mut elapsed: Local<Duration>,
    mut exit: MessageWriter<AppExit>,
) {
    *elapsed += time.delta();
    if *elapsed >= limit.0 {
        exit.write(AppExit::Success);
    }
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
