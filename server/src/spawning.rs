use bevy::prelude::*;

use shared::config::GameConfig;
use shared::network::protocol::prelude::{Direction, RoomId};

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
}
