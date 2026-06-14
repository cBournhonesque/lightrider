use bevy::prelude::*;
use lightyear::prelude::server::WebTransportServerIo;
use lightyear_matchmaker_bevy_server::{
    LightyearMatchmakerServerPlugin, MatchmakerServerState, NatsBridgeConfig,
};
use lightyear_matchmaker_core::{
    ProviderKind, RegisteredGameServer, ServerEndpoint, ServerId, ServerRoomMetrics,
};
use lightyear_matchmaker_nats::NatsConfig;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Instant;

use crate::rooms::RoomDirectory;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{RoomCode, RoomId};

const DEFAULT_GAME: &str = "lightrider";
const DEFAULT_VERSION: &str = "dev";
const DEFAULT_NAMESPACE: &str = "lightrider_dev";

pub(crate) fn matchmaker_server_plugin(
    port: u16,
    config: &GameConfig,
) -> LightyearMatchmakerServerPlugin {
    LightyearMatchmakerServerPlugin::new(registered_server(port))
        .with_capacity_limits(
            max_players(config),
            config.rooms.max_rooms.max(1).try_into().unwrap_or(u32::MAX),
        )
        .with_nats_bridge(NatsBridgeConfig {
            nats: nats_config_from_env(),
            ..default()
        })
        .with_lightyear_netcode()
}

pub(crate) struct LightriderMatchmakerMetricsPlugin;

#[derive(Resource)]
struct MatchmakerPublishRuntime {
    readiness_published: bool,
    capacity_timer: Timer,
    cpu_sampler: ProcessCpuSampler,
}

#[derive(Resource)]
struct ProcessCpuSampler {
    previous: Option<ProcessCpuSample>,
    clock_ticks_per_second: f32,
    cpu_capacity_cores: f32,
}

#[derive(Clone, Copy, Debug)]
struct ProcessCpuSample {
    instant: Instant,
    process_ticks: u64,
}

impl Default for MatchmakerPublishRuntime {
    fn default() -> Self {
        Self {
            readiness_published: false,
            capacity_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            cpu_sampler: ProcessCpuSampler::default(),
        }
    }
}

impl Default for ProcessCpuSampler {
    fn default() -> Self {
        Self {
            previous: read_process_cpu_sample(),
            clock_ticks_per_second: clock_ticks_per_second(),
            cpu_capacity_cores: cpu_capacity_cores(),
        }
    }
}

impl ProcessCpuSampler {
    fn sample_percent(&mut self) -> Option<f32> {
        let current = read_process_cpu_sample()?;
        let previous = self.previous.replace(current)?;
        let elapsed_seconds = current
            .instant
            .duration_since(previous.instant)
            .as_secs_f32();
        if elapsed_seconds <= f32::EPSILON
            || self.clock_ticks_per_second <= 0.0
            || self.cpu_capacity_cores <= 0.0
        {
            return None;
        }
        let elapsed_ticks = current.process_ticks.checked_sub(previous.process_ticks)? as f32;
        let process_cpu_seconds = elapsed_ticks / self.clock_ticks_per_second;
        let cpu_percent = process_cpu_seconds / elapsed_seconds / self.cpu_capacity_cores * 100.0;
        Some(cpu_percent.clamp(0.0, 100.0))
    }
}

impl Plugin for LightriderMatchmakerMetricsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchmakerPublishRuntime>();
        app.add_systems(Update, (publish_readiness_once, publish_capacity_metrics));
    }
}

fn publish_readiness_once(
    webtransport_servers: Query<&WebTransportServerIo>,
    mut state: ResMut<MatchmakerServerState>,
    mut runtime: ResMut<MatchmakerPublishRuntime>,
) {
    if runtime.readiness_published {
        return;
    }
    let Some(cert_digest) = webtransport_servers
        .iter()
        .find_map(webtransport_certificate_digest)
    else {
        return;
    };
    state.set_ready(true, Some(cert_digest.clone()));
    let mut capacity = state.capacity().clone();
    capacity.ready = true;
    capacity.cert_digest = Some(cert_digest.clone());
    state.publish_capacity(capacity);
    runtime.readiness_published = true;
    info!(
        server_id = %state.server().server_id,
        cert_digest,
        "Lightyear Matchmaker readiness published"
    );
}

fn publish_capacity_metrics(
    time: Res<Time>,
    config: Res<GameConfig>,
    directory: Res<RoomDirectory>,
    mut state: ResMut<MatchmakerServerState>,
    mut runtime: ResMut<MatchmakerPublishRuntime>,
) {
    if !runtime.capacity_timer.tick(time.delta()).just_finished() {
        return;
    }

    let room_capacity = config.rooms.max_players_per_room.max(1) as u32;
    let rooms = directory
        .metrics()
        .map(|room| ServerRoomMetrics {
            key: room_metrics_key(room.game_room, room.private),
            private: room.private,
            players: room.human_count as u32,
            max_players: room_capacity,
        })
        .collect::<Vec<_>>();
    let total_players = rooms.iter().map(|room| room.players).sum();

    let mut capacity = state.capacity().clone();
    capacity.ready = runtime.readiness_published;
    capacity.total_players = total_players;
    capacity.max_players = max_players(&config);
    capacity.max_rooms = config.rooms.max_rooms.max(1) as u32;
    capacity.rooms = rooms;
    capacity.cpu_percent = runtime.cpu_sampler.sample_percent();
    state.publish_capacity(capacity);
}

fn registered_server(port: u16) -> RegisteredGameServer {
    let provider = provider_kind_from_env();
    let endpoint = ServerEndpoint {
        public_ip: public_ip_from_env(),
        port: public_port_from_env(port),
    };
    RegisteredGameServer {
        server_id: ServerId::new(server_id_from_env()),
        provider,
        endpoint,
        game: env_string("LIGHTRIDER_MATCHMAKER_GAME")
            .or_else(|| env_string("EDGEGAP_APP_NAME"))
            .unwrap_or_else(|| DEFAULT_GAME.to_string()),
        version: env_string("LIGHTRIDER_MATCHMAKER_VERSION")
            .or_else(|| env_string("EDGEGAP_APP_VERSION"))
            .unwrap_or_else(|| DEFAULT_VERSION.to_string()),
        region: env_string("LIGHTRIDER_MATCHMAKER_REGION")
            .or_else(|| env_string("BEVYGAP_DEPLOYMENT_REGION")),
        metadata: server_metadata_from_env(),
    }
}

fn nats_config_from_env() -> NatsConfig {
    NatsConfig {
        url: nats_url_from_env(),
        username: env_string("NATS_USER"),
        password: env_string("NATS_PASSWORD"),
        namespace: Some(
            env_string("LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE")
                .or_else(|| env_string("MATCHMAKER_NATS_NAMESPACE"))
                .or_else(|| env_string("BEVYGAP_NATS_NAMESPACE"))
                .unwrap_or_else(|| DEFAULT_NAMESPACE.to_string()),
        ),
        ..default()
    }
}

fn nats_url_from_env() -> String {
    if let Some(url) = env_string("LIGHTYEAR_MATCHMAKER_NATS_URL")
        .or_else(|| env_string("MATCHMAKER_NATS_URL"))
        .or_else(|| env_string("NATS_URL"))
    {
        return url;
    }
    let host = env_string("NATS_HOST").unwrap_or_else(|| "127.0.0.1:4222".to_string());
    if host.contains("://") {
        return host;
    }
    format!("nats://{host}")
}

fn provider_kind_from_env() -> ProviderKind {
    match env_string("LIGHTRIDER_MATCHMAKER_PROVIDER")
        .or_else(|| env_string("BEVYGAP_DEPLOYMENT_PROVIDER"))
        .unwrap_or_else(|| {
            if std::env::var_os("ARBITRIUM_REQUEST_ID").is_some() {
                "edgegap".to_string()
            } else {
                "static".to_string()
            }
        })
        .to_ascii_lowercase()
        .as_str()
    {
        "edgegap" => ProviderKind::Edgegap,
        "gameflow" => ProviderKind::Gameflow,
        _ => ProviderKind::Static,
    }
}

fn server_id_from_env() -> String {
    env_string("LIGHTRIDER_SERVER_ID")
        .or_else(|| env_string("ARBITRIUM_REQUEST_ID"))
        .or_else(|| env_string("EDGEGAP_REQUEST_ID"))
        .unwrap_or_else(|| "lightrider-local".to_string())
}

fn public_ip_from_env() -> IpAddr {
    env_string("LIGHTRIDER_PUBLIC_IP")
        .or_else(|| env_string("ARBITRIUM_PUBLIC_IP"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

fn public_port_from_env(local_port: u16) -> u16 {
    env_string("LIGHTRIDER_PUBLIC_PORT")
        .and_then(|value| value.parse().ok())
        .or_else(|| edgegap_external_port_from_env(local_port))
        .unwrap_or(local_port)
}

fn edgegap_external_port_from_env(local_port: u16) -> Option<u16> {
    let mapping = env_string("ARBITRIUM_PORTS_MAPPING")?;
    let value = serde_json::from_str::<serde_json::Value>(&mapping).ok()?;
    value.as_object()?.values().find_map(|port| {
        let internal = port.get("internal")?.as_u64()?;
        if internal != u64::from(local_port) {
            return None;
        }
        port.get("external")?.as_u64()?.try_into().ok()
    })
}

fn server_metadata_from_env() -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    if let Some(country) = env_string("LIGHTRIDER_MATCHMAKER_COUNTRY")
        .or_else(|| env_string("BEVYGAP_DEPLOYMENT_COUNTRY_CODE"))
    {
        metadata.insert("country".to_string(), country);
    }
    metadata
}

fn max_players(config: &GameConfig) -> u32 {
    config
        .rooms
        .max_rooms
        .max(1)
        .saturating_mul(config.rooms.max_players_per_room.max(1)) as u32
}

fn webtransport_certificate_digest(server: &WebTransportServerIo) -> Option<String> {
    server
        .certificate
        .certificate_chain()
        .as_slice()
        .first()
        .map(|certificate| certificate.hash().to_string())
}

fn room_metrics_key(room: RoomId, private: bool) -> String {
    if private {
        if let Some(code) = RoomCode::from_room_id(room) {
            return format!("code:{code}");
        }
    }
    format!("id:{}", room.0)
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn read_process_cpu_sample() -> Option<ProcessCpuSample> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    Some(ProcessCpuSample {
        instant: Instant::now(),
        process_ticks: process_cpu_ticks_from_stat(&stat)?,
    })
}

#[cfg(not(target_os = "linux"))]
fn read_process_cpu_sample() -> Option<ProcessCpuSample> {
    None
}

#[cfg(target_os = "linux")]
fn clock_ticks_per_second() -> f32 {
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 {
        ticks as f32
    } else {
        100.0
    }
}

#[cfg(not(target_os = "linux"))]
fn clock_ticks_per_second() -> f32 {
    100.0
}

fn cpu_capacity_cores() -> f32 {
    cgroup_cpu_quota_cores()
        .or_else(|| {
            std::thread::available_parallelism()
                .ok()
                .map(|parallelism| parallelism.get() as f32)
        })
        .unwrap_or(1.0)
        .max(0.01)
}

#[cfg(target_os = "linux")]
fn cgroup_cpu_quota_cores() -> Option<f32> {
    std::fs::read_to_string("/sys/fs/cgroup/cpu.max")
        .ok()
        .and_then(|contents| cpu_quota_from_cpu_max(&contents))
        .or_else(|| {
            let quota = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us").ok()?;
            let period = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us").ok()?;
            cpu_quota_from_cfs(&quota, &period)
        })
}

#[cfg(not(target_os = "linux"))]
fn cgroup_cpu_quota_cores() -> Option<f32> {
    None
}

fn cpu_quota_from_cpu_max(contents: &str) -> Option<f32> {
    let mut fields = contents.split_whitespace();
    let quota = fields.next()?;
    if quota == "max" {
        return None;
    }
    let quota = quota.parse::<f32>().ok()?;
    let period = fields.next()?.parse::<f32>().ok()?;
    cpu_quota_from_values(quota, period)
}

fn cpu_quota_from_cfs(quota: &str, period: &str) -> Option<f32> {
    let quota = quota.trim().parse::<f32>().ok()?;
    let period = period.trim().parse::<f32>().ok()?;
    cpu_quota_from_values(quota, period)
}

fn cpu_quota_from_values(quota: f32, period: f32) -> Option<f32> {
    if quota <= 0.0 || period <= 0.0 {
        return None;
    }
    Some(quota / period)
}

fn process_cpu_ticks_from_stat(stat: &str) -> Option<u64> {
    let stat_after_command = stat.rsplit_once(") ")?.1;
    let fields = stat_after_command.split_whitespace().collect::<Vec<_>>();
    let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
    let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
    Some(user_ticks.saturating_add(system_ticks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn room_metrics_use_public_and_private_keys() {
        let code = RoomCode::parse("ABCD").unwrap();
        assert_eq!(room_metrics_key(code.room_id(), true), "code:ABCD");
        assert_eq!(room_metrics_key(RoomId(42), false), "id:42");
    }

    #[test]
    fn nats_config_uses_legacy_credentials() {
        with_env(
            [
                ("LIGHTYEAR_MATCHMAKER_NATS_URL", None::<&str>),
                ("MATCHMAKER_NATS_URL", None::<&str>),
                ("NATS_URL", None::<&str>),
                ("NATS_HOST", Some("127.0.0.1:4222")),
                ("NATS_USER", Some("lightrider")),
                ("NATS_PASSWORD", Some("secret")),
            ],
            || {
                assert_eq!(nats_url_from_env(), "nats://127.0.0.1:4222");
                let config = nats_config_from_env();
                assert_eq!(config.username.as_deref(), Some("lightrider"));
                assert_eq!(config.password.as_deref(), Some("secret"));
            },
        );
    }

    #[test]
    fn edgegap_port_mapping_uses_matching_internal_port() {
        with_env(
            [(
                "ARBITRIUM_PORTS_MAPPING",
                Some(r#"{"game":{"internal":7777,"external":30210,"protocol":"UDP"}}"#),
            )],
            || assert_eq!(edgegap_external_port_from_env(7777), Some(30210)),
        );
    }

    #[test]
    fn cpu_max_quota_parses_fractional_cores() {
        assert_eq!(cpu_quota_from_cpu_max("50000 100000\n"), Some(0.5));
        assert_eq!(cpu_quota_from_cpu_max("max 100000\n"), None);
    }

    #[test]
    fn process_cpu_ticks_parse_proc_stat_with_spaces_in_command() {
        let stat = "123 (lightrider server) S 1 2 3 4 5 6 7 8 9 10 200 30 0 0 20 0 1 0";
        assert_eq!(process_cpu_ticks_from_stat(stat), Some(230));
    }

    fn with_env<const N: usize>(vars: [(&str, Option<&str>); N], f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous = vars
            .iter()
            .map(|(key, _)| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in vars {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        f();
        for (key, value) in previous {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}
