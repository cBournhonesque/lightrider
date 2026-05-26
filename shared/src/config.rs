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
        app.register_type::<FoodConfig>();
        app.register_type::<RoomConfig>();
        app.register_type::<BotConfig>();
        app.register_type::<FakeClientConfig>();
        app.register_type::<NetworkConfig>();
        app.register_type::<DebugConfig>();
    }
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
#[serde(default)]
pub struct GameConfig {
    pub arena: ArenaConfig,
    pub movement: MovementConfig,
    pub food: FoodConfig,
    pub rooms: RoomConfig,
    pub bots: BotConfig,
    pub fake_clients: FakeClientConfig,
    pub network: NetworkConfig,
    pub debug: DebugConfig,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            arena: ArenaConfig::default(),
            movement: MovementConfig::default(),
            food: FoodConfig::default(),
            rooms: RoomConfig::default(),
            bots: BotConfig::default(),
            fake_clients: FakeClientConfig::default(),
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
pub struct FoodConfig {
    pub target_count: usize,
    pub spawn_interval_seconds: f32,
    pub radius: f32,
    pub tail_growth: f32,
}

impl Default for FoodConfig {
    fn default() -> Self {
        Self {
            target_count: 100,
            spawn_interval_seconds: 1.0,
            radius: 20.0,
            tail_growth: 20.0,
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
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_count_per_room: 0,
            decision_interval_ticks: 10,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct FakeClientConfig {
    pub enabled: bool,
    pub count: usize,
    pub input_interval_ticks: u32,
}

impl Default for FakeClientConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            count: 0,
            input_interval_ticks: 10,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct NetworkConfig {
    pub server_port: u16,
    pub artificial_latency_ms: u64,
    pub artificial_jitter_ms: u64,
    pub artificial_loss_percent: u8,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            server_port: 5000,
            artificial_latency_ms: 0,
            artificial_jitter_ms: 0,
            artificial_loss_percent: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Reflect)]
#[serde(default)]
pub struct DebugConfig {
    pub lightyear_debug: bool,
    pub json_snapshots: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            lightyear_debug: false,
            json_snapshots: false,
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
        assert_eq!(config.rooms.max_players_per_room, 50);
        assert_eq!(config.network.server_port, 5000);
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
