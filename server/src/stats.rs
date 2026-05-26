use bevy::prelude::*;
use shared::movement::SimulationSet;
use shared::network::protocol::prelude::{Player, PlayerRank, PlayerStats, PlayerStatus, Speed};

pub(crate) struct ServerStatsPlugin;

impl Plugin for ServerStatsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            update_player_life_stats.after(SimulationSet::Movement),
        );
    }
}

fn update_player_life_stats(
    time: Res<Time>,
    mut players: Query<(&Player, &PlayerStatus, &PlayerRank, &mut PlayerStats)>,
    speeds: Query<&Speed>,
) {
    let delta_seconds = time.delta_secs();
    for (player, status, rank, mut stats) in &mut players {
        if *status != PlayerStatus::Alive {
            continue;
        }
        let Some(snake) = player.snake else {
            continue;
        };

        stats.time_alive_seconds += delta_seconds;
        if rank.value == 1 {
            stats.time_as_leader_seconds += delta_seconds;
        }
        if let Ok(speed) = speeds.get(snake) {
            stats.record_speed(speed.0);
        }
    }
}
