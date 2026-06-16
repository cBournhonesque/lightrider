use anyhow::{anyhow, bail, Context as _};
use base64::prelude::BASE64_STANDARD;
use base64::Engine as _;
use bevy::prelude::*;
use lightyear::netcode::auth::Authentication;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::{ConnectToken, NetcodeClient};
use lightyear::prelude::client::WebTransportClientIo;
use lightyear::prelude::*;
use lightyear_matchmaker_bevy_client::{
    ConnectionGrantReady, MatchmakerClientFailed, RequestPlay,
};
#[cfg(not(target_arch = "wasm32"))]
use lightyear_matchmaker_bevy_client::{
    LightyearMatchmakerClientPlugin, MatchmakerClientConfig,
};
#[cfg(target_arch = "wasm32")]
use lightyear_matchmaker_bevy_client::{
    request_play_once, MatchmakerClientErrorInfo, MatchmakerClientResult,
};
use lightyear_matchmaker_core::{
    ConnectionGrant, ConnectionGrantKind, RoomSelection as MatchmakerRoomSelection,
};
use shared::network::protocol::prelude::RoomJoinMode;
use std::net::SocketAddr;
#[cfg(target_arch = "wasm32")]
use std::sync::{mpsc, Mutex};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

use crate::network::config::normalize_certificate_digest;

pub(crate) struct LightriderMatchmakerPlugin {
    pub(crate) config: LightriderMatchmakerConfig,
}

#[derive(Clone, Debug, Resource)]
pub(crate) struct LightriderMatchmakerConfig {
    pub(crate) matchmaker_url: String,
    pub(crate) game_name: String,
    pub(crate) game_version: String,
    pub(crate) room: RoomJoinMode,
}

#[derive(Clone, Debug, Default, Resource, PartialEq, Eq)]
pub(crate) enum LightriderMatchmakerState {
    #[default]
    Dormant,
    Waiting(String),
    Connecting,
    Finished,
    Error(String),
}

impl Plugin for LightriderMatchmakerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone())
            .init_resource::<LightriderMatchmakerState>();

        #[cfg(not(target_arch = "wasm32"))]
        {
            app.add_plugins(LightyearMatchmakerClientPlugin::new(
                MatchmakerClientConfig::new(self.config.matchmaker_url.clone()),
            ))
            .add_systems(Startup, request_matchmaker_assignment)
            .add_systems(
                Update,
                (connect_matchmaker_assignment, handle_matchmaker_failures),
            );
        }

        #[cfg(target_arch = "wasm32")]
        {
            app.add_message::<ConnectionGrantReady>()
                .add_message::<MatchmakerClientFailed>()
                .add_systems(Startup, request_matchmaker_assignment_once)
                .add_systems(
                    Update,
                    (
                        drain_matchmaker_assignment_once,
                        connect_matchmaker_assignment,
                        handle_matchmaker_failures,
                    )
                        .chain(),
                );
        }
    }
}

fn build_matchmaker_request(config: &LightriderMatchmakerConfig) -> RequestPlay {
    let mut request = RequestPlay::new(config.game_name.clone(), config.game_version.clone());
    request.room = matchmaker_room_selection(config.room);
    request
}

fn log_matchmaker_request(config: &LightriderMatchmakerConfig, request: &RequestPlay) {
    info!(
        matchmaker_url = %config.matchmaker_url,
        game = %config.game_name,
        version = %config.game_version,
        room = ?request.room,
        "requesting matchmaker assignment"
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn request_matchmaker_assignment(
    config: Res<LightriderMatchmakerConfig>,
    mut requests: MessageWriter<RequestPlay>,
    mut state: ResMut<LightriderMatchmakerState>,
) {
    let request = build_matchmaker_request(&config);
    log_matchmaker_request(&config, &request);
    requests.write(request);
    *state = LightriderMatchmakerState::Waiting("Waiting to connect to server".to_string());
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource)]
struct MatchmakerOneShotRuntime {
    receiver: Mutex<mpsc::Receiver<MatchmakerOneShotResult>>,
}

#[cfg(target_arch = "wasm32")]
enum MatchmakerOneShotResult {
    Ready(MatchmakerClientResult),
    Failed(String),
}

#[cfg(target_arch = "wasm32")]
fn request_matchmaker_assignment_once(
    mut commands: Commands,
    config: Res<LightriderMatchmakerConfig>,
    mut state: ResMut<LightriderMatchmakerState>,
) {
    let request = build_matchmaker_request(&config);
    log_matchmaker_request(&config, &request);

    let (sender, receiver) = mpsc::channel();
    commands.insert_resource(MatchmakerOneShotRuntime {
        receiver: Mutex::new(receiver),
    });

    let websocket_url = config.matchmaker_url.clone();
    spawn_local(async move {
        let result = match request_play_once(websocket_url, request).await {
            Ok(result) => MatchmakerOneShotResult::Ready(result),
            Err(error) => MatchmakerOneShotResult::Failed(error.to_string()),
        };
        let _ = sender.send(result);
    });

    *state = LightriderMatchmakerState::Waiting("Waiting to connect to server".to_string());
}

#[cfg(target_arch = "wasm32")]
fn drain_matchmaker_assignment_once(
    runtime: Option<Res<MatchmakerOneShotRuntime>>,
    mut ready: MessageWriter<ConnectionGrantReady>,
    mut failed: MessageWriter<MatchmakerClientFailed>,
) {
    let Some(runtime) = runtime else {
        return;
    };
    let receiver = match runtime.receiver.lock() {
        Ok(receiver) => receiver,
        Err(poisoned) => poisoned.into_inner(),
    };
    while let Ok(result) = receiver.try_recv() {
        match result {
            MatchmakerOneShotResult::Ready(result) => {
                ready.write(ConnectionGrantReady { result });
            }
            MatchmakerOneShotResult::Failed(message) => {
                failed.write(MatchmakerClientFailed {
                    message: message.clone(),
                    error: MatchmakerClientErrorInfo {
                        code: None,
                        message,
                        retryable: true,
                    },
                });
            }
        }
    }
}

fn connect_matchmaker_assignment(
    mut commands: Commands,
    mut ready: MessageReader<ConnectionGrantReady>,
    clients: Query<Entity, With<Client>>,
    mut state: ResMut<LightriderMatchmakerState>,
) {
    for message in ready.read() {
        *state = LightriderMatchmakerState::Connecting;
        match connect_lightyear_client(&mut commands, &clients, &message.result.grant) {
            Ok(()) => {
                info!(
                    assignment_id = message.result.assignment_id.as_deref(),
                    "Got matchmaker response; connecting to server"
                );
                *state = LightriderMatchmakerState::Finished;
            }
            Err(error) => {
                warn!("failed to apply matchmaker grant: {error:#}");
                *state = LightriderMatchmakerState::Error(error.to_string());
            }
        }
    }
}

fn handle_matchmaker_failures(
    mut failures: MessageReader<MatchmakerClientFailed>,
    mut state: ResMut<LightriderMatchmakerState>,
) {
    for failure in failures.read() {
        warn!("matchmaker request failed: {}", failure.message);
        *state = LightriderMatchmakerState::Error(failure.message.clone());
    }
}

fn connect_lightyear_client(
    commands: &mut Commands,
    clients: &Query<Entity, With<Client>>,
    grant: &ConnectionGrant,
) -> anyhow::Result<()> {
    let connect_info = LightyearConnectInfo::from_grant(grant)?;
    let mut iter = clients.iter();
    let Some(client) = iter.next() else {
        bail!("no Lightyear Client entity exists");
    };
    if iter.next().is_some() {
        warn!(
            "multiple Lightyear Client entities found; using the first one for matchmaker connect"
        );
    }

    let netcode_config = NetcodeConfig {
        client_timeout_secs: 3,
        token_expire_secs: -1,
        ..default()
    };
    let netcode = NetcodeClient::new(Authentication::Token(connect_info.token), netcode_config)
        .map_err(|error| anyhow!("failed to build Lightyear Netcode client: {error:?}"))?;
    commands.entity(client).insert((
        PeerAddr(connect_info.server_addr),
        WebTransportClientIo {
            certificate_digest: connect_info.certificate_digest,
        },
        netcode,
    ));
    commands.trigger(Connect { entity: client });
    Ok(())
}

struct LightyearConnectInfo {
    token: ConnectToken,
    server_addr: SocketAddr,
    certificate_digest: String,
}

impl LightyearConnectInfo {
    fn from_grant(grant: &ConnectionGrant) -> anyhow::Result<Self> {
        if grant.kind != ConnectionGrantKind::LightyearNetcode {
            bail!("unsupported connection grant kind: {:?}", grant.kind);
        }
        let token_bytes = BASE64_STANDARD
            .decode(&grant.token)
            .context("failed to decode Lightyear connect token")?;
        let token = ConnectToken::try_from_bytes(&token_bytes)
            .map_err(|error| anyhow!("invalid Lightyear connect token: {error:?}"))?;
        let certificate_digest = grant
            .cert_digest
            .as_deref()
            .map(normalize_certificate_digest)
            .unwrap_or_default();
        Ok(Self {
            token,
            server_addr: grant.endpoint.socket_addr(),
            certificate_digest,
        })
    }
}

fn matchmaker_room_selection(room: RoomJoinMode) -> MatchmakerRoomSelection {
    match room {
        RoomJoinMode::Auto => MatchmakerRoomSelection::Auto,
        RoomJoinMode::New => MatchmakerRoomSelection::New,
        RoomJoinMode::Specific(room_id) => MatchmakerRoomSelection::Id(room_id.0.to_string()),
        RoomJoinMode::Private(code) => MatchmakerRoomSelection::Code(code.to_string()),
    }
}
