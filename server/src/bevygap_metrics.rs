use bevy::prelude::*;
use bevygap_server_plugin::prelude::{BevygapDeploymentMetrics, DeploymentRoomMetrics};
use std::time::Instant;

use crate::rooms::RoomDirectory;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{RoomCode, RoomId};

pub(crate) struct ServerBevygapMetricsPlugin;

#[derive(Resource)]
struct DeploymentMetricsTimer(Timer);

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

impl Default for DeploymentMetricsTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1.0, TimerMode::Repeating))
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

impl Plugin for ServerBevygapMetricsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeploymentMetricsTimer>();
        app.init_resource::<ProcessCpuSampler>();
        app.add_systems(Update, publish_deployment_metrics);
    }
}

fn publish_deployment_metrics(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<GameConfig>,
    directory: Res<RoomDirectory>,
    mut timer: ResMut<DeploymentMetricsTimer>,
    mut cpu_sampler: ResMut<ProcessCpuSampler>,
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
        cpu_percent: cpu_sampler.sample_percent(),
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

    #[test]
    fn private_room_metrics_use_room_code_key() {
        let code = RoomCode::parse("ABCD").unwrap();
        assert_eq!(room_metrics_key(code.room_id(), true), "code:ABCD");
    }

    #[test]
    fn public_room_metrics_use_numeric_key() {
        assert_eq!(room_metrics_key(RoomId(42), false), "id:42");
    }

    #[test]
    fn process_cpu_ticks_parse_proc_stat_with_spaces_in_command() {
        let stat = "123 (lightrider server) S 1 2 3 4 5 6 7 8 9 10 200 30 0 0 20 0 1 0";
        assert_eq!(process_cpu_ticks_from_stat(stat), Some(230));
    }

    #[test]
    fn cpu_max_quota_parses_fractional_cores() {
        assert_eq!(cpu_quota_from_cpu_max("50000 100000\n"), Some(0.5));
        assert_eq!(cpu_quota_from_cpu_max("200000 100000\n"), Some(2.0));
        assert_eq!(cpu_quota_from_cpu_max("max 100000\n"), None);
    }

    #[test]
    fn cfs_quota_parses_fractional_cores() {
        assert_eq!(cpu_quota_from_cfs("25000\n", "100000\n"), Some(0.25));
        assert_eq!(cpu_quota_from_cfs("-1\n", "100000\n"), None);
    }
}
