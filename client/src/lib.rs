use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{App, PluginGroup, ScheduleRunnerPlugin};
use bevy::asset::{AssetMetaCheck, AssetPlugin};
use bevy::diagnostic::DiagnosticsPlugin;
use bevy::input::InputPlugin;
use bevy::prelude::default;
use bevy::state::app::StatesPlugin;
use bevy::transform::TransformPlugin;
use bevy::window::{Window, WindowPlugin};
use bevy::{DefaultPlugins, MinimalPlugins};
use clap::{Parser, ValueEnum};
#[cfg(feature = "lightyear-matchmaker")]
use lightyear_matchmaker_core::ProviderKind;

use shared::config::GameConfig;
use shared::debug::{runtime_log_plugin, RuntimeDebugPlugin};
use shared::network::protocol::prelude::RoomJoinMode;
use shared::SharedPlugin;

mod admin;
mod camera;
mod collision;
mod debug;
mod food;
mod inputs;
mod leaderboard;
#[cfg(feature = "lightyear-matchmaker")]
mod matchmaker;
mod menu;
pub(crate) mod network;
mod render;
mod rooms;
mod sound;
#[cfg(all(target_family = "wasm", feature = "lightyear-matchmaker"))]
mod web_status;

// Use a port of 0 to automatically select a port
pub const CLIENT_PORT: u16 = 0;

pub const SERVER_PORT: u16 = 5000;

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClientMode {
    Player,
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

    /// Automatically request an initial spawn and later respawns when allowed.
    #[arg(long, default_value = "false")]
    auto_respawn: bool,

    /// Generate local player turn inputs at this rate for headless RTT stress tests.
    #[arg(long, default_value_t = 0.0)]
    turn_stress_hz: f32,

    /// Stop generating turn-stress inputs after this many seconds. Zero means no limit.
    #[arg(long, default_value_t = 0.0)]
    turn_stress_seconds: f32,

    /// Record browser RTT samples into window.__lightriderRttSamples for deployed web tests.
    #[arg(long, default_value = "false")]
    browser_rtt_probe: bool,

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

    /// Use Lightyear Matchmaker instead of direct server address connection.
    #[cfg(feature = "lightyear-matchmaker")]
    #[arg(long)]
    matchmaker_url: Option<String>,

    /// Game name sent to the matchmaker.
    #[cfg(feature = "lightyear-matchmaker")]
    #[arg(long, default_value = "lightrider")]
    matchmaker_game: String,

    /// Game version sent to the matchmaker.
    #[cfg(feature = "lightyear-matchmaker")]
    #[arg(long, default_value = "dev")]
    matchmaker_version: String,

    /// Exact matchmaker provider to request: static, edgegap, or gameflow.
    #[cfg(feature = "lightyear-matchmaker")]
    #[arg(long, value_parser = parse_matchmaker_provider)]
    matchmaker_provider: Option<ProviderKind>,

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

#[cfg(feature = "lightyear-matchmaker")]
#[derive(Clone, Debug)]
pub struct WebClientOptions {
    pub matchmaker_url: String,
    pub matchmaker_game: String,
    pub matchmaker_version: String,
    pub matchmaker_provider: Option<ProviderKind>,
    pub room: RoomJoinMode,
    pub name: String,
    pub canvas_selector: String,
    pub headless: bool,
    pub auto_respawn: bool,
    pub turn_stress_hz: f32,
    pub turn_stress_seconds: f32,
    pub browser_rtt_probe: bool,
}

#[cfg(feature = "lightyear-matchmaker")]
impl Cli {
    pub fn web_defaults(matchmaker_url: String) -> Self {
        Self {
            inspector: false,
            debug: false,
            headless: false,
            auto_respawn: false,
            turn_stress_hz: 0.0,
            turn_stress_seconds: 0.0,
            browser_rtt_probe: false,
            mode: ClientMode::Player,
            client_id: 0,
            client_port: CLIENT_PORT,
            cert_digest: String::new(),
            matchmaker_url: Some(matchmaker_url),
            matchmaker_game: "lightrider".to_string(),
            matchmaker_version: "dev".to_string(),
            matchmaker_provider: None,
            server_addr: Ipv4Addr::LOCALHOST,
            server_port: SERVER_PORT,
            room: RoomJoinMode::Auto,
            name: String::new(),
            config: None,
            canvas_selector: Some("#bevy_canvas".to_string()),
        }
    }
}

#[cfg(feature = "lightyear-matchmaker")]
pub fn web_app(options: WebClientOptions) -> App {
    let mut cli = Cli::web_defaults(options.matchmaker_url);
    cli.matchmaker_game = options.matchmaker_game;
    cli.matchmaker_version = options.matchmaker_version;
    cli.matchmaker_provider = options.matchmaker_provider;
    cli.room = options.room;
    cli.name = options.name;
    cli.canvas_selector = Some(options.canvas_selector);
    cli.headless = options.headless;
    cli.auto_respawn = options.auto_respawn;
    cli.turn_stress_hz = options.turn_stress_hz;
    cli.turn_stress_seconds = options.turn_stress_seconds;
    cli.browser_rtt_probe = options.browser_rtt_probe;
    app(cli)
}

pub fn app(cli: Cli) -> App {
    let mut app = App::new();
    let config = load_config(cli.config.as_deref());
    #[cfg(target_family = "wasm")]
    let config = {
        let mut config = config;
        if cli.browser_rtt_probe || cli.turn_stress_hz > 0.0 {
            config.debug.lightyear_debug = true;
        }
        config
    };
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
        let mut plugins = DefaultPlugins.set(log_plugin).set(AssetPlugin {
            file_path: asset_file_path(),
            // Browser asset hosts can return HTML/error bodies for missing `.meta`
            // sidecars, which Bevy then tries to parse as RON.
            meta_check: AssetMetaCheck::Never,
            ..default()
        });
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
    }

    #[cfg(feature = "lightyear-matchmaker")]
    let matchmaker_config =
        cli.matchmaker_url
            .as_ref()
            .map(|matchmaker_url| matchmaker::LightriderMatchmakerConfig {
                matchmaker_url: matchmaker_url.clone(),
                game_name: cli.matchmaker_game.clone(),
                game_version: cli.matchmaker_version.clone(),
                provider: cli.matchmaker_provider,
                room: cli.room,
            });

    #[cfg(feature = "lightyear-matchmaker")]
    let network_connection = if matchmaker_config.is_some() {
        network::config::ClientConnectionConfig::matchmaker(cli.client_port)
    } else {
        network::config::ClientConnectionConfig::direct(
            cli.client_id,
            cli.client_port,
            (cli.server_addr, cli.server_port).into(),
            cli.cert_digest.clone(),
        )
    };

    #[cfg(not(feature = "lightyear-matchmaker"))]
    let network_connection = network::config::ClientConnectionConfig::direct(
        cli.client_id,
        cli.client_port,
        (cli.server_addr, cli.server_port).into(),
        cli.cert_digest.clone(),
    );

    app.add_plugins(network::NetworkPlugin {
        connection: network_connection,
    });
    #[cfg(feature = "lightyear-matchmaker")]
    if let Some(config) = matchmaker_config {
        app.add_plugins(matchmaker::LightriderMatchmakerPlugin { config });
    }
    app.add_plugins(SharedPlugin);
    app.add_plugins(RuntimeDebugPlugin::client());
    app.add_plugins(collision::CollisionPlugin);
    app.add_plugins(food::PredictedFoodPlugin);
    app.add_plugins(leaderboard::ClientLeaderboardPlugin);
    app.add_plugins(rooms::ClientRoomsPlugin {
        mode: cli.room,
        name: player_name,
    });
    if cli.auto_respawn {
        app.insert_resource(network::inputs::AutoRespawnRequests);
    }
    if cli.turn_stress_hz > 0.0 {
        app.insert_resource(network::inputs::TurnStressSettings {
            hz: cli.turn_stress_hz,
            duration_seconds: if cli.turn_stress_seconds > 0.0 {
                Some(cli.turn_stress_seconds)
            } else {
                None
            },
        });
    }
    #[cfg(target_family = "wasm")]
    if cli.browser_rtt_probe || cli.turn_stress_hz > 0.0 {
        app.insert_resource(network::inputs::BrowserRttProbe);
    }
    if !cli.headless {
        app.add_plugins(inputs::LocalInputsPlugin { debug_enabled });
        app.add_plugins(camera::CameraPlugin { debug_enabled });
        app.add_plugins(debug::DebugPlugin);
        app.add_plugins(admin::ClientAdminPlugin);
        app.add_plugins(render::RenderPlugin);
        app.add_plugins(sound::SoundPlugin);
        #[cfg(all(target_family = "wasm", feature = "lightyear-matchmaker"))]
        app.add_plugins(web_status::WebStatusPlugin);
    }
    app
}

fn asset_file_path() -> String {
    if cfg!(target_family = "wasm") {
        "assets".to_string()
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("client crate should live under the workspace root")
            .join("assets")
            .to_string_lossy()
            .into_owned()
    }
}

fn player_name(cli: &Cli) -> String {
    let trimmed = cli.name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    match cli.mode {
        ClientMode::Player => format!("Player {}", cli.client_id),
    }
}

#[cfg(feature = "lightyear-matchmaker")]
fn parse_matchmaker_provider(value: &str) -> Result<ProviderKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "static" => Ok(ProviderKind::Static),
        "edgegap" => Ok(ProviderKind::Edgegap),
        "gameflow" => Ok(ProviderKind::Gameflow),
        _ => Err("expected `static`, `edgegap`, or `gameflow`".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_clients_load_assets_from_workspace_root() {
        assert!(std::path::Path::new(&asset_file_path())
            .join("powerline/sheet.png")
            .exists());
    }
}
