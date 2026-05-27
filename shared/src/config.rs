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
        app.register_type::<FoodConfig>();
        app.register_type::<RoomConfig>();
        app.register_type::<BotConfig>();
        app.register_type::<FakeClientConfig>();
        app.register_type::<RespawnConfig>();
        app.register_type::<NetworkConfig>();
        app.register_type::<InputDelayConfig>();
        app.register_type::<DebugConfig>();
    }
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct GameConfig {
    pub arena: ArenaConfig,
    pub movement: MovementConfig,
    pub render: RenderConfig,
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
            tick_rate_hz: 30.0,
            starting_tail_length: 200.0,
            min_speed: 1.0,
            max_speed: 4.0,
            base_acceleration: -0.01,
            food_boost_acceleration: 0.08,
            food_boost_decay: 0.85,
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
    pub tail_width: f32,
    pub head_size: f32,
    pub map_outline_width: f32,
    pub normal_camera_scale: f32,
    pub normal_camera_growth_per_tail_length: f32,
    pub normal_camera_max_scale: f32,
    pub debug_camera_scale: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            tail_width: 3.0,
            head_size: 10.0,
            map_outline_width: 3.0,
            normal_camera_scale: 0.35,
            normal_camera_growth_per_tail_length: 0.001,
            normal_camera_max_scale: 1.0,
            debug_camera_scale: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct FoodConfig {
    pub target_count: usize,
    pub spawn_interval_seconds: f32,
    pub visual_radius: f32,
    pub radius: f32,
    pub magnet_radius: f32,
    pub magnet_speed: f32,
    pub tail_growth: f32,
    pub death_food_spacing: f32,
    pub death_food_max: usize,
}

impl Default for FoodConfig {
    fn default() -> Self {
        Self {
            target_count: 100,
            spawn_interval_seconds: 1.0,
            visual_radius: 3.0,
            radius: 8.0,
            magnet_radius: 75.0,
            magnet_speed: 10.0,
            tail_growth: 20.0,
            death_food_spacing: 28.0,
            death_food_max: 40,
        }
    }
}

impl FoodConfig {
    pub fn spawn_interval(&self) -> Duration {
        Duration::from_secs_f32(self.spawn_interval_seconds)
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
            player_cooldown_seconds: 1.0,
            bot_cooldown_seconds: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct NetworkConfig {
    pub server_port: u16,
    pub input_delay: InputDelayConfig,
    pub input_packet_redundancy_ticks: u16,
    pub artificial_latency_ms: u64,
    pub artificial_jitter_ms: u64,
    pub artificial_loss_percent: u8,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            server_port: 5000,
            input_delay: InputDelayConfig::default(),
            input_packet_redundancy_ticks: 3,
            artificial_latency_ms: 0,
            artificial_jitter_ms: 0,
            artificial_loss_percent: 0,
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
        Self {
            minimum_input_delay_ticks: 0,
            maximum_input_delay_before_prediction_ticks: 3,
            maximum_predicted_ticks: 7,
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
        assert_eq!(config.render.tail_width, 3.0);
        assert_eq!(config.render.map_outline_width, 3.0);
        assert_eq!(config.render.normal_camera_scale, 0.35);
        assert_eq!(config.render.normal_camera_max_scale, 1.0);
        assert_eq!(config.render.debug_camera_scale, 1.0);
        assert_eq!(config.food.visual_radius, 3.0);
        assert_eq!(config.food.radius, 8.0);
        assert_eq!(config.food.magnet_radius, 75.0);
        assert_eq!(config.food.death_food_max, 40);
        assert_eq!(config.movement.food_boost_acceleration, 0.08);
        assert_eq!(config.movement.food_boost_decay, 0.85);
        assert_eq!(config.rooms.max_players_per_room, 50);
        assert_eq!(config.bots.mistake_chance_per_decision_percent, 3);
        assert_eq!(config.fake_clients.mistake_chance_per_decision_percent, 3);
        assert_eq!(config.network.server_port, 5000);
        assert_eq!(
            config.network.input_delay,
            InputDelayConfig {
                minimum_input_delay_ticks: 0,
                maximum_input_delay_before_prediction_ticks: 3,
                maximum_predicted_ticks: 7,
            }
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
    }

    #[test]
    fn movement_tick_duration_uses_configured_rate() {
        let movement = MovementConfig {
            tick_rate_hz: 30.0,
            ..default()
        };
        assert!((movement.tick_duration().as_secs_f32() - (1.0 / 30.0)).abs() < f32::EPSILON);

        let invalid = MovementConfig {
            tick_rate_hz: 0.0,
            ..default()
        };
        assert_eq!(
            invalid.tick_duration(),
            MovementConfig::default().tick_duration()
        );
    }
}
