use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{App, PluginGroup, ScheduleRunnerPlugin};
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::input::InputPlugin;
use bevy::log::{Level, LogPlugin};
use bevy::state::app::StatesPlugin;
use bevy::transform::TransformPlugin;
use bevy::{DefaultPlugins, MinimalPlugins};
use clap::{Parser, ValueEnum};

use shared::config::GameConfig;
use shared::network::protocol::prelude::RoomJoinMode;
use shared::SharedPlugin;

mod bot;
mod camera;
mod collision;
mod debug;
mod inputs;
mod menu;
pub(crate) mod network;
mod render;
mod rooms;

// Use a port of 0 to automatically select a port
pub const CLIENT_PORT: u16 = 0;

pub const SERVER_PORT: u16 = 5000;

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClientMode {
    Player,
    Bot,
}

#[derive(Parser, PartialEq, Debug)]
pub struct Cli {
    #[arg(short, long, default_value = "false")]
    inspector: bool,

    #[arg(long, default_value = "false")]
    headless: bool,

    #[arg(long, value_enum, default_value_t = ClientMode::Player)]
    mode: ClientMode,

    #[arg(short, long, default_value_t = 0)]
    client_id: u64,

    #[arg(long, default_value_t = CLIENT_PORT)]
    client_port: u16,

    #[arg(long, default_value_t = Ipv4Addr::LOCALHOST)]
    server_addr: Ipv4Addr,

    #[arg(short, long, default_value_t = SERVER_PORT)]
    server_port: u16,

    /// Hex SHA-256 digest of the WebTransport server certificate.
    ///
    /// Native local development can leave this empty because the dev build enables Lightyear's
    /// dangerous WebTransport configuration. Browser builds should pass the digest printed by the
    /// server, for example through deployment config or page bootstrap.
    #[arg(long, default_value = "")]
    certificate_digest: String,

    #[arg(long, default_value = "auto", value_parser = rooms::parse_room_join_mode)]
    room: RoomJoinMode,

    #[arg(long)]
    config: Option<PathBuf>,
}

pub fn app(cli: Cli) -> App {
    let mut app = App::new();
    let config = load_config(cli.config.as_deref());
    let bot_decision_interval_ticks = config.fake_clients.input_interval_ticks;
    app.insert_resource(config);
    if cli.headless {
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 60.0,
            ))),
            TransformPlugin,
            InputPlugin,
            StatesPlugin,
            DiagnosticsPlugin,
            LogPlugin {
                level: Level::INFO,
                filter: "wgpu=error,bevy_ecs=trace".to_string(),
                ..Default::default()
            },
        ));
    } else {
        app.add_plugins(DefaultPlugins.set(LogPlugin {
            level: Level::INFO,
            filter: "wgpu=error,bevy_render=info,bevy_ecs=trace".to_string(),
            ..Default::default()
        }));
    }

    app.add_plugins(SharedPlugin);
    app.add_plugins(network::NetworkPlugin {
        client_id: cli.client_id,
        client_port: cli.client_port,
        server_addr: (cli.server_addr, cli.server_port).into(),
        certificate_digest: cli.certificate_digest,
    });
    app.add_plugins(collision::CollisionPlugin);
    app.add_plugins(rooms::ClientRoomsPlugin { mode: cli.room });
    if cli.mode == ClientMode::Bot {
        app.add_plugins(bot::BotClientPlugin {
            decision_interval_ticks: bot_decision_interval_ticks,
        });
    }
    if !cli.headless {
        app.add_plugins(inputs::LocalInputsPlugin);
        app.add_plugins(camera::CameraPlugin);
        app.add_plugins(debug::DebugPlugin);
        app.add_plugins(render::RenderPlugin);
    }
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
