use std::time::Duration;

use anyhow::Context;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>();
        app.register_type::<GameConfig>();
        app.register_type::<ArenaConfig>();
        app.register_type::<MovementConfig>();
        app.register_type::<RenderConfig>();
        app.register_type::<SoundConfig>();
        app.register_type::<FoodConfig>();
        app.register_type::<RoomConfig>();
        app.register_type::<BotConfig>();
        app.register_type::<FakeClientConfig>();
        app.register_type::<RespawnConfig>();
        app.register_type::<NetworkConfig>();
        app.register_type::<NetworkCompression>();
        app.register_type::<InputDelayConfig>();
        app.register_type::<LagCompensationConfig>();
        app.register_type::<DebugConfig>();
    }
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct GameConfig {
    pub arena: ArenaConfig,
    pub movement: MovementConfig,
    pub render: RenderConfig,
    pub sound: SoundConfig,
    pub food: FoodConfig,
    pub rooms: RoomConfig,
    pub bots: BotConfig,
    pub fake_clients: FakeClientConfig,
    pub respawn: RespawnConfig,
    pub network: NetworkConfig,
    pub debug: DebugConfig,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            arena: ArenaConfig::default(),
            movement: MovementConfig::default(),
            render: RenderConfig::default(),
            sound: SoundConfig::default(),
            food: FoodConfig::default(),
            rooms: RoomConfig::default(),
            bots: BotConfig::default(),
            fake_clients: FakeClientConfig::default(),
            respawn: RespawnConfig::default(),
            network: NetworkConfig::default(),
            debug: DebugConfig::default(),
        }
    }
}

impl GameConfig {
    pub fn from_ron_str(source: &str) -> anyhow::Result<Self> {
        ron::de::from_str(source).context("failed to parse game config RON")
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn from_ron_file(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read game config {}", path.display()))?;
        Self::from_ron_str(&source)
            .with_context(|| format!("failed to load game config {}", path.display()))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct ArenaConfig {
    pub width: f32,
    pub height: f32,
}

impl Default for ArenaConfig {
    fn default() -> Self {
        Self {
            width: 5000.0,
            height: 1600.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct MovementConfig {
    pub tick_rate_hz: f32,
    pub starting_tail_length: f32,
    pub min_speed: f32,
    pub max_speed: f32,
    pub base_acceleration: f32,
    pub food_boost_acceleration: f32,
    pub food_boost_decay: f32,
    pub boost_acceleration_ratio: f32,
    pub boost_distance: f32,
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self {
            tick_rate_hz: 32.0,
            starting_tail_length: 200.0,
            min_speed: 0.85,
            max_speed: 4.0,
            base_acceleration: -0.01,
            food_boost_acceleration: 0.03,
            food_boost_decay: 0.94,
            boost_acceleration_ratio: 2.0,
            boost_distance: 20.0,
        }
    }
}

impl MovementConfig {
    pub fn tick_duration(&self) -> Duration {
        let tick_rate_hz = if self.tick_rate_hz > 0.0 {
            self.tick_rate_hz
        } else {
            Self::default().tick_rate_hz
        };
        Duration::from_secs_f32(1.0 / tick_rate_hz)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct RenderConfig {
    pub use_assets: bool,
    pub tail_width: f32,
    pub head_size: f32,
    pub map_outline_width: f32,
    pub background_tile_size: f32,
    pub normal_camera_scale: f32,
    pub normal_camera_growth_per_tail_length: f32,
    pub normal_camera_scale_smoothing: f32,
    pub normal_camera_max_scale: f32,
    pub debug_camera_scale: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            use_assets: true,
            tail_width: 1.0,
            head_size: 6.0,
            map_outline_width: 3.0,
            background_tile_size: 128.0,
            normal_camera_scale: 0.35,
            normal_camera_growth_per_tail_length: 0.00015,
            normal_camera_scale_smoothing: 6.0,
            normal_camera_max_scale: 0.62,
            debug_camera_scale: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct SoundConfig {
    pub enabled: bool,
    pub master_volume: f32,
    pub background_volume: f32,
    pub death_volume: f32,
    pub food_volume: f32,
    pub electro_loop_volume: f32,
    pub spatial_audio: bool,
    pub spatial_scale: f32,
    pub spatial_listener_ear_gap: f32,
    pub remote_sound_full_volume_distance: f32,
    pub remote_sound_max_distance: f32,
    pub remote_death_volume: f32,
    pub remote_food_volume: f32,
    pub remote_speed_volume: f32,
    pub speed_loop_start_speed: f32,
    pub speed_loop_min_volume: f32,
    pub speed_loop_max_volume: f32,
    pub speed_fast_loop_start_speed: f32,
    pub speed_fast_loop_volume: f32,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            master_volume: 1.0,
            background_volume: 0.0,
            death_volume: 0.55,
            food_volume: 0.65,
            electro_loop_volume: 0.85,
            spatial_audio: true,
            spatial_scale: 0.02,
            spatial_listener_ear_gap: 8.0,
            remote_sound_full_volume_distance: 120.0,
            remote_sound_max_distance: 900.0,
            remote_death_volume: 1.0,
            remote_food_volume: 0.75,
            remote_speed_volume: 0.6,
            speed_loop_start_speed: 1.2,
            speed_loop_min_volume: 0.12,
            speed_loop_max_volume: 1.0,
            speed_fast_loop_start_speed: 3.0,
            speed_fast_loop_volume: 1.4,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct FoodConfig {
    pub target_count: usize,
    pub max_count: usize,
    pub spawn_interval_seconds: f32,
    pub visual_radius: f32,
    pub radius: f32,
    pub tail_growth: f32,
    pub death_food_spacing: f32,
    pub death_food_max: usize,
}

impl Default for FoodConfig {
    fn default() -> Self {
        Self {
            target_count: 400,
            max_count: 600,
            spawn_interval_seconds: 0.05,
            visual_radius: 3.0,
            radius: 45.0,
            tail_growth: 20.0,
            death_food_spacing: 18.0,
            death_food_max: 120,
        }
    }
}

impl FoodConfig {
    pub fn spawn_interval(&self) -> Duration {
        Duration::from_secs_f32(self.spawn_interval_seconds)
    }

    pub fn spawn_target_count(&self) -> usize {
        self.target_count.min(self.max_count)
    }

    pub fn remaining_capacity(&self, current_count: usize) -> usize {
        self.max_count.saturating_sub(current_count)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct RoomConfig {
    pub max_rooms: usize,
    pub max_players_per_room: usize,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            max_rooms: 16,
            max_players_per_room: 50,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct BotConfig {
    pub enabled: bool,
    pub target_count_per_room: usize,
    pub decision_interval_ticks: u32,
    pub mistake_chance_per_decision_percent: u8,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_count_per_room: 0,
            decision_interval_ticks: 10,
            mistake_chance_per_decision_percent: 3,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct FakeClientConfig {
    pub enabled: bool,
    pub count: usize,
    pub input_interval_ticks: u32,
    pub mistake_chance_per_decision_percent: u8,
}

impl Default for FakeClientConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            count: 0,
            input_interval_ticks: 10,
            mistake_chance_per_decision_percent: 3,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct RespawnConfig {
    pub player_cooldown_seconds: f32,
    pub bot_cooldown_seconds: f32,
}

impl Default for RespawnConfig {
    fn default() -> Self {
        Self {
            player_cooldown_seconds: 3.0,
            bot_cooldown_seconds: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct NetworkConfig {
    pub server_port: u16,
    pub replication_send_hz: u16,
    pub compression: NetworkCompression,
    pub input_delay: InputDelayConfig,
    pub lag_compensation: LagCompensationConfig,
    pub input_packet_redundancy_ticks: u16,
    pub artificial_latency_ms: u64,
    pub artificial_jitter_ms: u64,
    pub artificial_loss_percent: u8,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            server_port: 5000,
            replication_send_hz: 16,
            compression: NetworkCompression::default(),
            input_delay: InputDelayConfig::default(),
            lag_compensation: LagCompensationConfig::default(),
            input_packet_redundancy_ticks: 3,
            artificial_latency_ms: 0,
            artificial_jitter_ms: 0,
            artificial_loss_percent: 0,
        }
    }
}

impl NetworkConfig {
    pub fn replication_send_interval(&self) -> Duration {
        let send_hz = if self.replication_send_hz > 0 {
            self.replication_send_hz
        } else {
            Self::default().replication_send_hz
        };
        Duration::from_nanos(1_000_000_000 / u64::from(send_hz))
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum NetworkCompression {
    #[default]
    Disabled,
    Lz4,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct LagCompensationConfig {
    pub enabled: bool,
    pub max_delay_ticks: u16,
}

impl Default for LagCompensationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_delay_ticks: 10,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct InputDelayConfig {
    pub minimum_input_delay_ticks: u16,
    pub maximum_input_delay_before_prediction_ticks: u16,
    pub maximum_predicted_ticks: u16,
}

impl Default for InputDelayConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

impl InputDelayConfig {
    /// Lightrider's balanced input-delay preset.
    ///
    /// At the default 32Hz simulation rate, this covers roughly 60ms with input delay before
    /// falling back to prediction. Higher latency predicts up to `maximum_predicted_ticks`.
    pub const fn balanced() -> Self {
        Self {
            minimum_input_delay_ticks: 0,
            maximum_input_delay_before_prediction_ticks: 2,
            maximum_predicted_ticks: 8,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct DebugConfig {
    pub lightyear_debug: bool,
    pub json_snapshots: bool,
    pub snake_trace_sample_interval_ticks: u32,
    pub invariant_checks: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            lightyear_debug: false,
            json_snapshots: false,
            snake_trace_sample_interval_ticks: 1,
            invariant_checks: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_file_matches_expected_powerline_shape() {
        let config = GameConfig::from_ron_str(include_str!("../../config/default.ron")).unwrap();

        assert_eq!(config.arena.width, 5000.0);
        assert_eq!(config.arena.height, 1600.0);
        assert_eq!(config.render.tail_width, 1.0);
        assert_eq!(config.render.map_outline_width, 3.0);
        assert_eq!(config.render.normal_camera_scale, 0.35);
        assert_eq!(config.render.normal_camera_growth_per_tail_length, 0.00015);
        assert_eq!(config.render.normal_camera_scale_smoothing, 6.0);
        assert_eq!(config.render.normal_camera_max_scale, 0.62);
        assert_eq!(config.render.debug_camera_scale, 1.0);
        assert_eq!(config.movement.min_speed, 0.85);
        assert!(config.sound.enabled);
        assert_eq!(config.sound.background_volume, 0.0);
        assert_eq!(config.sound.death_volume, 0.55);
        assert_eq!(config.sound.electro_loop_volume, 0.85);
        assert_eq!(config.sound.remote_sound_max_distance, 900.0);
        assert_eq!(config.sound.remote_speed_volume, 0.6);
        assert_eq!(config.sound.speed_loop_start_speed, 1.2);
        assert_eq!(config.sound.speed_loop_max_volume, 1.0);
        assert_eq!(config.food.visual_radius, 3.0);
        assert_eq!(config.food.radius, 45.0);
        assert_eq!(config.food.max_count, 600);
        assert_eq!(config.food.death_food_max, 120);
        assert_eq!(config.movement.food_boost_acceleration, 0.03);
        assert_eq!(config.movement.food_boost_decay, 0.94);
        assert_eq!(config.rooms.max_players_per_room, 50);
        assert_eq!(config.bots.mistake_chance_per_decision_percent, 3);
        assert_eq!(config.fake_clients.mistake_chance_per_decision_percent, 3);
        assert_eq!(config.network.server_port, 5000);
        assert_eq!(config.network.replication_send_hz, 16);
        assert_eq!(config.network.compression, NetworkCompression::Disabled);
        assert_eq!(
            config.network.replication_send_interval(),
            Duration::from_nanos(62_500_000)
        );
        assert_eq!(config.network.input_delay, InputDelayConfig::balanced());
        assert_eq!(
            config.network.lag_compensation,
            LagCompensationConfig::default()
        );
        assert_eq!(config.network.input_packet_redundancy_ticks, 3);
    }

    #[test]
    fn test_config_file_is_small_and_fast() {
        let config = GameConfig::from_ron_str(include_str!("../../config/test.ron")).unwrap();

        assert!(config.arena.width < GameConfig::default().arena.width);
        assert!(config.arena.height < GameConfig::default().arena.height);
        assert!(
            config.rooms.max_players_per_room < GameConfig::default().rooms.max_players_per_room
        );
        assert!(config.food.target_count < GameConfig::default().food.target_count);
        assert!(config.food.max_count < GameConfig::default().food.max_count);
    }

    #[test]
    fn load_config_file_is_sized_for_many_headless_clients() {
        let config = GameConfig::from_ron_str(include_str!("../../config/load.ron")).unwrap();

        assert!(config.rooms.max_players_per_room >= 100);
        assert!(!config.bots.enabled);
        assert!(!config.sound.enabled);
        assert!(config.debug.lightyear_debug);
        assert!(!config.debug.json_snapshots);
        assert!(config.food.max_count >= config.food.target_count);
    }

    #[test]
    fn no_food_load_config_disables_food_without_trace_snapshots() {
        let config =
            GameConfig::from_ron_str(include_str!("../../config/load_no_food.ron")).unwrap();

        assert!(config.rooms.max_players_per_room >= 100);
        assert!(!config.bots.enabled);
        assert!(!config.sound.enabled);
        assert!(config.debug.lightyear_debug);
        assert!(!config.debug.json_snapshots);
        assert_eq!(config.food.target_count, 0);
        assert_eq!(config.food.max_count, 0);
        assert_eq!(config.food.death_food_max, 0);
    }

    #[test]
    fn food_spawn_target_respects_max_count() {
        let mut food = FoodConfig {
            target_count: 400,
            max_count: 250,
            ..default()
        };

        assert_eq!(food.spawn_target_count(), 250);
        assert_eq!(food.remaining_capacity(249), 1);
        assert_eq!(food.remaining_capacity(250), 0);
        assert_eq!(food.remaining_capacity(300), 0);

        food.max_count = 0;
        assert_eq!(food.spawn_target_count(), 0);
    }

    #[test]
    fn movement_tick_duration_uses_configured_rate() {
        let movement = MovementConfig {
            tick_rate_hz: 32.0,
            ..default()
        };
        assert!((movement.tick_duration().as_secs_f32() - (1.0 / 32.0)).abs() < f32::EPSILON);

        let invalid = MovementConfig {
            tick_rate_hz: 0.0,
            ..default()
        };
        assert_eq!(
            invalid.tick_duration(),
            MovementConfig::default().tick_duration()
        );
    }

    #[test]
    fn replication_send_interval_uses_configured_rate() {
        let network = NetworkConfig {
            replication_send_hz: 16,
            ..default()
        };
        assert_eq!(
            network.replication_send_interval(),
            Duration::from_nanos(62_500_000)
        );

        let invalid = NetworkConfig {
            replication_send_hz: 0,
            ..default()
        };
        assert_eq!(
            invalid.replication_send_interval(),
            NetworkConfig::default().replication_send_interval()
        );
    }

    #[test]
    fn load_no_food_lz4_config_enables_transport_compression() {
        let config =
            GameConfig::from_ron_str(include_str!("../../config/load_no_food_lz4.ron")).unwrap();

        assert_eq!(config.network.compression, NetworkCompression::Lz4);
        assert_eq!(config.food.target_count, 0);
        assert_eq!(config.food.max_count, 0);
    }
}
