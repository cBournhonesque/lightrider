use bevy::prelude::*;
use bevygap_server_plugin::prelude::{BevygapDeploymentMetrics, DeploymentRoomMetrics};

use crate::rooms::RoomDirectory;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{RoomCode, RoomId};

pub(crate) struct ServerBevygapMetricsPlugin;

#[derive(Resource)]
struct DeploymentMetricsTimer(Timer);

impl Default for DeploymentMetricsTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1.0, TimerMode::Repeating))
    }
}

impl Plugin for ServerBevygapMetricsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeploymentMetricsTimer>();
        app.add_systems(Update, publish_deployment_metrics);
    }
}

fn publish_deployment_metrics(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<GameConfig>,
    directory: Res<RoomDirectory>,
    mut timer: ResMut<DeploymentMetricsTimer>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    let room_capacity = config.rooms.max_players_per_room.max(1) as u32;
    let rooms = directory
        .metrics()
        .map(|room| DeploymentRoomMetrics {
            key: room_metrics_key(room.game_room, room.private),
            private: room.private,
            players: room.human_count as u32,
            max_players: room_capacity,
        })
        .collect::<Vec<_>>();
    let total_players = rooms.iter().map(|room| room.players).sum();
    let max_rooms = config.rooms.max_rooms.max(1) as u32;
    let max_players = max_rooms.saturating_mul(room_capacity);

    commands.trigger(BevygapDeploymentMetrics {
        total_players,
        max_players,
        max_rooms,
        cpu_percent: None,
        rooms,
    });
}

fn room_metrics_key(room: RoomId, private: bool) -> String {
    if private {
        if let Some(code) = RoomCode::from_room_id(room) {
            return format!("code:{code}");
        }
    }
    format!("id:{}", room.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_room_metrics_use_room_code_key() {
        let code = RoomCode::parse("ABCD").unwrap();
        assert_eq!(room_metrics_key(code.room_id(), true), "code:ABCD");
    }

    #[test]
    fn public_room_metrics_use_numeric_key() {
        assert_eq!(room_metrics_key(RoomId(42), false), "id:42");
    }
}
