use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{App, PluginGroup, ScheduleRunnerPlugin};
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::input::InputPlugin;
use bevy::prelude::default;
use bevy::state::app::StatesPlugin;
use bevy::transform::TransformPlugin;
use bevy::window::{Window, WindowPlugin};
use bevy::{DefaultPlugins, MinimalPlugins};
use clap::{Parser, ValueEnum};

use shared::config::GameConfig;
use shared::debug::{runtime_log_plugin, RuntimeDebugPlugin};
use shared::network::protocol::prelude::RoomJoinMode;
use shared::SharedPlugin;

#[cfg(feature = "bevygap")]
use bevygap_client_plugin::prelude::{
    BevygapClientConfig, BevygapClientPlugin, BevygapConnectExt,
    RoomSelection as BevygapRoomSelection,
};

mod bot;
mod camera;
mod collision;
mod debug;
mod inputs;
mod menu;
pub(crate) mod network;
mod render;
mod rooms;
mod sound;
#[cfg(all(target_family = "wasm", feature = "bevygap"))]
mod web_status;

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
    /// Enable local debug tools. Kept as an alias for existing workflows.
    #[arg(short, long, default_value = "false")]
    inspector: bool,

    /// Enable local debug shortcuts such as camera zoom toggle and shortcut help.
    #[arg(long, default_value = "false")]
    debug: bool,

    /// Run without rendering, for bot clients and load-test clients.
    #[arg(long, default_value = "false")]
    headless: bool,

    #[arg(long, value_enum, default_value_t = ClientMode::Player)]
    mode: ClientMode,

    #[arg(short, long, default_value_t = 0)]
    client_id: u64,

    #[arg(long, default_value_t = CLIENT_PORT)]
    client_port: u16,

    /// Hex SHA-256 digest for the WebTransport server certificate.
    ///
    /// Native local clients may leave this empty when using the dangerous WebTransport
    /// test configuration. Browser clients must get this from matchmaking.
    #[arg(long, alias = "certificate-digest", default_value = "")]
    cert_digest: String,

    /// Use Bevygap matchmaking instead of direct server address connection.
    #[cfg(feature = "bevygap")]
    #[arg(long)]
    matchmaker_url: Option<String>,

    /// Bevygap game name sent to the matchmaker.
    #[cfg(feature = "bevygap")]
    #[arg(long, default_value = "lightrider")]
    matchmaker_game: String,

    /// Bevygap game version sent to the matchmaker.
    #[cfg(feature = "bevygap")]
    #[arg(long, default_value = "dev")]
    matchmaker_version: String,

    /// Override client IP sent to Bevygap matchmaker, useful for local testing.
    #[cfg(feature = "bevygap")]
    #[arg(long)]
    matchmaker_fake_client_ip: Option<String>,

    #[arg(long, default_value_t = Ipv4Addr::LOCALHOST)]
    server_addr: Ipv4Addr,

    #[arg(short, long, default_value_t = SERVER_PORT)]
    server_port: u16,

    #[arg(long, default_value = "auto", value_parser = rooms::parse_room_join_mode)]
    room: RoomJoinMode,

    #[arg(long, default_value = "")]
    name: String,

    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(skip)]
    canvas_selector: Option<String>,
}

#[cfg(feature = "bevygap")]
#[derive(Clone, Debug)]
pub struct WebClientOptions {
    pub matchmaker_url: String,
    pub matchmaker_game: String,
    pub matchmaker_version: String,
    pub room: RoomJoinMode,
    pub name: String,
    pub canvas_selector: String,
}

#[cfg(feature = "bevygap")]
impl Cli {
    pub fn web_defaults(matchmaker_url: String) -> Self {
        Self {
            inspector: false,
            debug: false,
            headless: false,
            mode: ClientMode::Player,
            client_id: 0,
            client_port: CLIENT_PORT,
            cert_digest: String::new(),
            matchmaker_url: Some(matchmaker_url),
            matchmaker_game: "lightrider".to_string(),
            matchmaker_version: "dev".to_string(),
            matchmaker_fake_client_ip: None,
            server_addr: Ipv4Addr::LOCALHOST,
            server_port: SERVER_PORT,
            room: RoomJoinMode::Auto,
            name: String::new(),
            config: None,
            canvas_selector: Some("#bevy_canvas".to_string()),
        }
    }
}

#[cfg(feature = "bevygap")]
pub fn web_app(options: WebClientOptions) -> App {
    let mut cli = Cli::web_defaults(options.matchmaker_url);
    cli.matchmaker_game = options.matchmaker_game;
    cli.matchmaker_version = options.matchmaker_version;
    cli.room = options.room;
    cli.name = options.name;
    cli.canvas_selector = Some(options.canvas_selector);
    app(cli)
}

pub fn app(cli: Cli) -> App {
    let mut app = App::new();
    let config = load_config(cli.config.as_deref());
    let bot_decision_interval_ticks = config.fake_clients.input_interval_ticks;
    let bot_mistake_chance_per_decision_percent =
        config.fake_clients.mistake_chance_per_decision_percent;
    let player_name = player_name(&cli);
    let debug_enabled = cli.debug || cli.inspector;
    let log_plugin = if cli.headless {
        runtime_log_plugin(&config, "wgpu=error,bevy_ecs=trace")
    } else {
        runtime_log_plugin(&config, "wgpu=error,bevy_render=info,bevy_ecs=trace")
    };
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
            log_plugin,
        ));
    } else {
        let mut plugins = DefaultPlugins.set(log_plugin);
        if let Some(canvas_selector) = cli.canvas_selector.clone() {
            plugins = plugins.set(WindowPlugin {
                primary_window: Some(Window {
                    canvas: Some(canvas_selector),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: false,
                    ..default()
                }),
                ..default()
            });
        }
        app.add_plugins(plugins);
        #[cfg(target_family = "wasm")]
        app.add_plugins(bevy_web_keepalive::WebKeepalivePlugin::default());
    }

    #[cfg(feature = "bevygap")]
    let bevygap_config = cli
        .matchmaker_url
        .as_ref()
        .map(|matchmaker_url| BevygapClientConfig {
            matchmaker_url: matchmaker_url.clone(),
            fake_client_ip: cli.matchmaker_fake_client_ip.clone(),
            game_name: cli.matchmaker_game.clone(),
            game_version: cli.matchmaker_version.clone(),
            room: bevygap_room_selection(cli.room),
        });

    #[cfg(feature = "bevygap")]
    let network_connection = if bevygap_config.is_some() {
        network::config::ClientConnectionConfig::bevygap(cli.client_port)
    } else {
        network::config::ClientConnectionConfig::direct(
            cli.client_id,
            cli.client_port,
            (cli.server_addr, cli.server_port).into(),
            cli.cert_digest.clone(),
        )
    };

    #[cfg(not(feature = "bevygap"))]
    let network_connection = network::config::ClientConnectionConfig::direct(
        cli.client_id,
        cli.client_port,
        (cli.server_addr, cli.server_port).into(),
        cli.cert_digest.clone(),
    );

    app.add_plugins(network::NetworkPlugin {
        connection: network_connection,
    });
    #[cfg(feature = "bevygap")]
    if let Some(bevygap_config) = bevygap_config {
        app.insert_resource(bevygap_config);
        app.add_plugins(BevygapClientPlugin);
        app.add_systems(bevy::prelude::Startup, request_bevygap_session);
    }
    app.add_plugins(SharedPlugin);
    app.add_plugins(RuntimeDebugPlugin::client());
    app.add_plugins(collision::CollisionPlugin);
    app.add_plugins(rooms::ClientRoomsPlugin {
        mode: cli.room,
        name: player_name,
    });
    if cli.mode == ClientMode::Bot {
        app.add_plugins(bot::BotClientPlugin {
            decision_interval_ticks: bot_decision_interval_ticks,
            mistake_chance_per_decision_percent: bot_mistake_chance_per_decision_percent,
        });
    }
    if !cli.headless {
        app.add_plugins(inputs::LocalInputsPlugin { debug_enabled });
        app.add_plugins(camera::CameraPlugin { debug_enabled });
        app.add_plugins(debug::DebugPlugin);
        app.add_plugins(render::RenderPlugin);
        app.add_plugins(sound::SoundPlugin);
        #[cfg(all(target_family = "wasm", feature = "bevygap"))]
        app.add_plugins(web_status::WebStatusPlugin);
    }
    app
}

#[cfg(feature = "bevygap")]
fn request_bevygap_session(mut commands: bevy::prelude::Commands) {
    commands.bevygap_connect_client();
}

#[cfg(feature = "bevygap")]
fn bevygap_room_selection(room: RoomJoinMode) -> BevygapRoomSelection {
    match room {
        RoomJoinMode::Auto => BevygapRoomSelection::Auto,
        RoomJoinMode::New => BevygapRoomSelection::New,
        RoomJoinMode::Specific(room_id) => BevygapRoomSelection::Id(room_id.0.to_string()),
        RoomJoinMode::Private(code) => BevygapRoomSelection::Code(code.to_string()),
    }
}

fn player_name(cli: &Cli) -> String {
    let trimmed = cli.name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    match cli.mode {
        ClientMode::Player => format!("Player {}", cli.client_id),
        ClientMode::Bot => format!("Bot Client {}", cli.client_id),
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
