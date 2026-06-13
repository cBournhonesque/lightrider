use bevy::prelude::*;

use shared::config::GameConfig;
use shared::network::protocol::prelude::{Direction, RoomId, TailPolyline};
use shared::utils::geometry::{project_on_segment, ray_segment_intersection};

const SPAWN_ATTEMPTS: u64 = 32;

pub(crate) fn snake_spawn_pose(
    config: &GameConfig,
    room: RoomId,
    entity_seed: u64,
) -> (Vec2, Direction) {
    let mut seed = entity_seed ^ room.0.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let x = normalized_hash(&mut seed) * config.arena.width * 0.35;
    let y = normalized_hash(&mut seed) * config.arena.height * 0.35;
    let direction = match next_hash(&mut seed) % 4 {
        0 => Direction::Up,
        1 => Direction::Right,
        2 => Direction::Down,
        _ => Direction::Left,
    };
    (Vec2::new(x, y), direction)
}

pub(crate) fn snake_spawn_pose_avoiding<'a>(
    config: &GameConfig,
    room: RoomId,
    entity_seed: u64,
    obstacle_tails: impl IntoIterator<Item = &'a TailPolyline>,
) -> (Vec2, Direction) {
    let obstacle_tails = obstacle_tails.into_iter().collect::<Vec<_>>();
    let mut best = None;
    let required_clearance = spawn_clearance(config);

    for attempt in 0..SPAWN_ATTEMPTS {
        let seed = entity_seed ^ attempt.wrapping_mul(0xd1b5_4a32_d192_ed03);
        let pose = snake_spawn_pose(config, room, seed);
        let score = spawn_score(config, pose.0, pose.1, &obstacle_tails);
        if score >= required_clearance {
            return pose;
        }
        if best.map_or(true, |(best_score, _)| score > best_score) {
            best = Some((score, pose));
        }
    }

    best.map(|(_, pose)| pose)
        .unwrap_or_else(|| snake_spawn_pose(config, room, entity_seed))
}

fn spawn_clearance(config: &GameConfig) -> f32 {
    (config.movement.starting_tail_length * 0.75).clamp(60.0, 160.0)
}

fn spawn_score(
    config: &GameConfig,
    position: Vec2,
    direction: Direction,
    obstacle_tails: &[&TailPolyline],
) -> f32 {
    let tail_end = position - direction.delta() * config.movement.starting_tail_length;
    let boundary_distance =
        arena_clearance(position, config).min(arena_clearance(tail_end, config));
    if boundary_distance <= 0.0 {
        return 0.0;
    }

    obstacle_tails
        .iter()
        .flat_map(|tail| tail.pairs_front_to_back())
        .map(|(segment_start, segment_end)| {
            let segment_start = segment_start.0;
            let segment_end = segment_end.0;
            if ray_segment_intersection(
                tail_end,
                direction.delta(),
                config.movement.starting_tail_length,
                segment_start,
                segment_end,
            )
            .is_some()
            {
                0.0
            } else {
                point_segment_distance(position, segment_start, segment_end)
                    .min(point_segment_distance(tail_end, segment_start, segment_end))
            }
        })
        .fold(boundary_distance, f32::min)
}

fn arena_clearance(position: Vec2, config: &GameConfig) -> f32 {
    let half_width = config.arena.width * 0.5;
    let half_height = config.arena.height * 0.5;
    (half_width - position.x.abs()).min(half_height - position.y.abs())
}

fn point_segment_distance(point: Vec2, segment_start: Vec2, segment_end: Vec2) -> f32 {
    point.distance(project_on_segment(&segment_start, &segment_end, &point))
}

fn next_hash(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed
}

fn normalized_hash(seed: &mut u64) -> f32 {
    let value = (next_hash(seed) >> 32) as u32;
    (value as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_pose_stays_inside_inner_arena() {
        let config = GameConfig::default();
        for seed in 0..100 {
            let (position, _) = snake_spawn_pose(&config, RoomId(0), seed);
            assert!(position.x.abs() <= config.arena.width * 0.35);
            assert!(position.y.abs() <= config.arena.height * 0.35);
        }
    }

    #[test]
    fn spawn_score_rejects_intersecting_initial_tail() {
        use std::collections::VecDeque;

        let config = GameConfig::default();
        let obstacle = TailPolyline::new(VecDeque::from([
            (Vec2::new(-50.0, -100.0), Direction::Right),
            (Vec2::new(50.0, -100.0), Direction::Right),
        ]));

        assert_eq!(
            spawn_score(&config, Vec2::ZERO, Direction::Up, &[&obstacle]),
            0.0
        );
    }

    #[test]
    fn spawn_score_accepts_clear_initial_tail() {
        use std::collections::VecDeque;

        let config = GameConfig::default();
        let obstacle = TailPolyline::new(VecDeque::from([
            (Vec2::new(-50.0, 500.0), Direction::Right),
            (Vec2::new(50.0, 500.0), Direction::Right),
        ]));

        assert!(spawn_score(&config, Vec2::ZERO, Direction::Up, &[&obstacle]) >= 160.0);
    }
}
