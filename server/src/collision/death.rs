use crate::food::spawn_food_entity;
use crate::respawn::{respawn_delay_seconds, RespawnReadyAt};
use crate::rooms::{remove_replicated_entity_from_room, ClientRoom, RoomDirectory};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use lightyear::prelude::{
    server::ClientOf, NetworkTarget, RemoteId, Server, ServerMultiMessageSender,
};
use shared::bot::BotMarker;
use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;
use tracing::{debug, error};

pub struct DeathPlugin;

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RoomDirectory>();
        app.add_systems(
            FixedUpdate,
            handle_collision.after(ColliderSet::ComputeCollision),
        );
    }
}

pub fn handle_collision(
    mut reader: MessageReader<SnakeCollision>,
    mut sender: ServerMultiMessageSender,
    server: Single<&Server>,
    rooms: Res<RoomDirectory>,
    config: Res<GameConfig>,
    time: Res<Time>,
    mut players: ParamSet<(
        Query<(&Player, &PlayerScore, &PlayerStats)>,
        Query<&mut PlayerStats>,
        Query<(&mut Player, &mut PlayerStatus, Has<BotMarker>)>,
    )>,
    clients: Query<(&RemoteId, &ClientRoom), With<ClientOf>>,
    snakes: Query<(&HasPlayer, &RoomId, &SnakeHead, &TailPoints, &TailLength)>,
    food: Query<&RoomId, With<FoodMarker>>,
    mut commands: Commands,
) {
    let server = server.into_inner();
    let mut killed_snakes = EntityHashSet::default();
    let mut room_food_counts = room_food_counts(&food);
    for collision_event in reader.read() {
        if !reserve_collision_death(&mut killed_snakes, collision_event) {
            continue;
        }
        let Ok((killed_player, killed_room, killed_head, killed_tail, killed_length)) =
            snakes.get(collision_event.killed)
        else {
            error!("snake does not have HasPlayer component");
            continue;
        };
        let Ok((killer_player, killer_room, _, _, _)) = snakes.get(collision_event.killer) else {
            error!("snake does not have HasPlayer component");
            continue;
        };
        if killed_room != killer_room {
            error!(?collision_event, "snake collision crossed room boundaries");
            continue;
        }
        let Ok((killed_name, killed_stats)) = ({
            let player_read = players.p0();
            player_read
                .get(killed_player.0)
                .map(|(player, score, stats)| {
                    (
                        player.name.clone(),
                        PlayerDeathStats::from_live(score.value, stats),
                    )
                })
        }) else {
            error!("killed player could not be found");
            continue;
        };
        let Ok(killer_name) = ({
            let player_read = players.p0();
            player_read
                .get(killer_player.0)
                .map(|(player, _, _)| player.name.clone())
        }) else {
            error!("killer player could not be found");
            continue;
        };

        if collision_event.reason == DeathReason::Collision && killer_player.0 != killed_player.0 {
            if let Ok(mut killer_stats) = players.p1().get_mut(killer_player.0) {
                killer_stats.kills = killer_stats.kills.saturating_add(1);
            }
        };
        debug!(?collision_event, "Collision event!");

        let death_message = PlayerDeath {
            killer_player: killer_player.0,
            killed_player: killed_player.0,
            killer_snake: collision_event.killer,
            killed_snake: collision_event.killed,
            killer_name,
            killed_name,
            room: *killed_room,
            reason: collision_event.reason,
            position: killed_head.position,
            stats: killed_stats,
        };
        for (remote_id, client_room) in &clients {
            if client_room.room != *killed_room {
                continue;
            }
            let _ = sender
                .send::<_, GameChannel>(&death_message, server, &NetworkTarget::Single(remote_id.0))
                .map_err(|e| error!(?e, "Failed to send death message"));
        }

        // despawn dead snake and remove snake from player
        if let Some(lightyear_room) = rooms.lightyear_room(*killed_room) {
            remove_replicated_entity_from_room(
                &mut commands,
                lightyear_room,
                collision_event.killed,
            );
        }
        commands.entity(collision_event.killed).try_despawn();
        spawn_death_food(
            &mut commands,
            &rooms,
            &config,
            *killed_room,
            &killed_tail.polyline(killed_head, killed_length.current_size),
            &mut room_food_counts,
        );
        let killed_is_bot = {
            let mut player_states = players.p2();
            let Ok((mut killed, mut killed_status, killed_is_bot)) =
                player_states.get_mut(killed_player.0)
            else {
                error!("player could not be found");
                continue;
            };
            killed.snake = None;
            *killed_status = PlayerStatus::Dead;
            killed_is_bot
        };
        commands
            .entity(killed_player.0)
            .insert(RespawnReadyAt::from_now(
                time.elapsed_secs_f64(),
                respawn_delay_seconds(&config, killed_is_bot),
            ));
    }
}

fn reserve_collision_death(
    killed_snakes: &mut EntityHashSet,
    collision_event: &SnakeCollision,
) -> bool {
    if collision_event.killer != collision_event.killed
        && killed_snakes.contains(&collision_event.killer)
    {
        return false;
    }
    killed_snakes.insert(collision_event.killed)
}

fn room_food_counts(
    food: &Query<&RoomId, With<FoodMarker>>,
) -> std::collections::HashMap<RoomId, usize> {
    let mut counts = std::collections::HashMap::new();
    for room in food {
        *counts.entry(*room).or_insert(0) += 1;
    }
    counts
}

fn spawn_death_food(
    commands: &mut Commands,
    rooms: &RoomDirectory,
    config: &GameConfig,
    room: RoomId,
    tail: &TailPolyline,
    room_food_counts: &mut std::collections::HashMap<RoomId, usize>,
) {
    let current_food_count = room_food_counts.get(&room).copied().unwrap_or_default();
    let available_slots = death_food_spawn_limit(config, current_food_count);
    if available_slots == 0 {
        return;
    }
    for position in death_food_positions(
        tail,
        config.food.death_food_spacing,
        config.food.death_food_max.min(available_slots),
    ) {
        spawn_food_entity(commands, rooms, room, Position(position));
        *room_food_counts.entry(room).or_insert(0) += 1;
    }
}

fn death_food_spawn_limit(config: &GameConfig, current_food_count: usize) -> usize {
    config
        .food
        .death_food_max
        .min(config.food.remaining_capacity(current_food_count))
}

pub fn death_food_positions(tail: &TailPolyline, spacing: f32, max_food: usize) -> Vec<Vec2> {
    if spacing <= 0.0 || max_food == 0 {
        return Vec::new();
    }
    let total_length = tail.total_length();
    if total_length <= f32::EPSILON {
        return Vec::new();
    }

    let food_count = ((total_length / spacing).floor() as usize)
        .max(1)
        .min(max_food);
    let sample_step = total_length / food_count as f32;
    let jitter_radius = (spacing * 0.18).min(5.0);
    let mut positions = Vec::with_capacity(food_count);
    for index in 0..food_count {
        let distance = sample_step * (index as f32 + 0.5);
        if let Some(position) = tail_position_at_distance(tail, distance) {
            positions
                .push(position + deterministic_death_food_jitter(position, index, jitter_radius));
        }
    }
    positions
}

fn tail_position_at_distance(tail: &TailPolyline, distance: f32) -> Option<Vec2> {
    let mut remaining = distance.max(0.0);
    for (start, end) in tail.pairs_front_to_back() {
        let segment = end.0 - start.0;
        let length = segment.length();
        if length <= f32::EPSILON {
            continue;
        }
        if remaining <= length {
            return Some(start.0 + segment / length * remaining);
        }
        remaining -= length;
    }
    tail.0.back().map(|(point, _)| *point)
}

fn deterministic_death_food_jitter(position: Vec2, index: usize, radius: f32) -> Vec2 {
    if radius <= 0.0 {
        return Vec2::ZERO;
    }
    let hash = mix_death_food_hash(
        position.x.to_bits() as u64
            ^ (position.y.to_bits() as u64).rotate_left(21)
            ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
    );
    let angle = unit_float(hash) * std::f32::consts::TAU;
    let distance = radius * (0.25 + 0.75 * unit_float(hash.rotate_left(17)));
    Vec2::from_angle(angle) * distance
}

fn mix_death_food_hash(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn unit_float(value: u64) -> f32 {
    ((value >> 40) as f32) / ((1_u64 << 24) as f32)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    #[test]
    fn death_food_samples_tail_segments_without_exceeding_limit() {
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::new(100.0, 0.0), Direction::Right),
            (Vec2::ZERO, Direction::Right),
        ]));

        let positions = death_food_positions(&tail, 25.0, 3);

        assert_eq!(positions.len(), 3);
        assert!((positions[0].x - 16.7).abs() < 6.0);
        assert!((positions[1].x - 50.0).abs() < 6.0);
        assert!((positions[2].x - 83.3).abs() < 6.0);
        assert!(positions.iter().all(|position| position.y.abs() <= 5.0));
    }

    #[test]
    fn death_food_limit_is_distributed_across_full_tail() {
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::new(300.0, 0.0), Direction::Right),
            (Vec2::ZERO, Direction::Right),
        ]));

        let positions = death_food_positions(&tail, 25.0, 3);

        assert_eq!(positions.len(), 3);
        assert!(positions[0].x < 70.0);
        assert!(positions[2].x > 230.0);
    }

    #[test]
    fn death_food_spawn_limit_respects_room_food_capacity() {
        let mut config = GameConfig::default();
        config.food.max_count = 10;
        config.food.death_food_max = 4;

        assert_eq!(death_food_spawn_limit(&config, 0), 4);
        assert_eq!(death_food_spawn_limit(&config, 8), 2);
        assert_eq!(death_food_spawn_limit(&config, 10), 0);
        assert_eq!(death_food_spawn_limit(&config, 12), 0);
    }

    #[test]
    fn reciprocal_same_tick_collisions_do_not_kill_both_snakes() {
        let mut world = World::new();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        let mut killed = EntityHashSet::default();

        assert!(reserve_collision_death(
            &mut killed,
            &SnakeCollision {
                killer: second,
                killed: first,
                reason: DeathReason::Collision,
            },
        ));
        assert!(!reserve_collision_death(
            &mut killed,
            &SnakeCollision {
                killer: first,
                killed: second,
                reason: DeathReason::Collision,
            },
        ));

        assert!(killed.contains(&first));
        assert!(!killed.contains(&second));
    }
}
