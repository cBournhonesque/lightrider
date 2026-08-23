use bevy::prelude::*;
use lightyear::prelude::{LocalTimeline, PeerId, RoomId as LightyearRoomId};
use std::collections::HashMap;

use crate::bot::{BotController, BotMarker};
use crate::respawn::RespawnReadyAt;
use crate::rooms::{add_replicated_entity_to_room, RoomAssignment, RoomDirectory};
use crate::spawning::snake_spawn_pose_avoiding;
use shared::config::GameConfig;
use shared::movement::{
    turn_rate_limit_window_ticks, turn_tail_with_diff_limited, SimulationSet, TurnRateLimiter,
};
use shared::network::bundle::player::PlayerBundle;
use shared::network::bundle::snake::SnakeBundle;
use shared::network::protocol::prelude::*;

const FIRST_BOT_ID: u64 = 10_000;

#[derive(Resource, Debug)]
struct BotIdAllocator {
    next: u64,
}

impl Default for BotIdAllocator {
    fn default() -> Self {
        Self { next: FIRST_BOT_ID }
    }
}

impl BotIdAllocator {
    fn next(&mut self) -> PeerId {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        PeerId::Netcode(id)
    }
}

#[derive(Resource, Debug, Default)]
pub(crate) struct BotTargetOverrides {
    per_room: HashMap<RoomId, usize>,
}

impl BotTargetOverrides {
    pub(crate) fn set_target(&mut self, config: &GameConfig, room: RoomId, count: usize) -> usize {
        let count = clamp_bot_target(config, count, 0);
        self.per_room.insert(room, count);
        count
    }

    pub(crate) fn target_for_room(
        &self,
        config: &GameConfig,
        room: RoomId,
        human_count: usize,
    ) -> usize {
        if human_count == 0 {
            return 0;
        }
        let target = if let Some(override_target) = self.per_room.get(&room).copied() {
            override_target
        } else if config.bots.enabled {
            config.bots.target_count_per_room.max(
                config
                    .bots
                    .minimum_total_players_per_room
                    .saturating_sub(human_count),
            )
        } else {
            0
        };
        clamp_bot_target(config, target, human_count)
    }

    fn is_empty(&self) -> bool {
        self.per_room.is_empty()
    }
}

pub(crate) struct ServerBotsPlugin;

impl Plugin for ServerBotsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BotIdAllocator>();
        app.init_resource::<BotTargetOverrides>();
        app.add_systems(Update, maintain_bots);
        app.add_systems(FixedUpdate, drive_bots.before(SimulationSet::Movement));
    }
}

fn maintain_bots(
    mut commands: Commands,
    config: Res<GameConfig>,
    directory: Res<RoomDirectory>,
    targets: Res<BotTargetOverrides>,
    time: Res<Time>,
    mut ids: ResMut<BotIdAllocator>,
    tails: Query<(&SnakeHead, &TailPoints, Option<&TailLength>, &RoomId)>,
    mut bot_queries: ParamSet<(
        Query<(Entity, &Player, &RoomId), (With<Player>, With<BotMarker>)>,
        Query<
            (
                Entity,
                &mut Player,
                &mut PlayerScore,
                &mut PlayerStats,
                &mut PlayerStatus,
                &RoomId,
                Option<&RespawnReadyAt>,
            ),
            With<BotMarker>,
        >,
    )>,
) {
    if !config.bots.enabled && targets.is_empty() {
        return;
    }

    let bot_snapshots = bot_queries
        .p0()
        .iter()
        .map(|(entity, player, room)| (entity, player.id, player.snake, *room))
        .collect::<Vec<_>>();
    let rooms = directory
        .metrics()
        .map(|metrics| (metrics.game_room, metrics.human_count))
        .collect::<HashMap<_, _>>();
    let assignments = directory.iter().collect::<Vec<_>>();
    for room in &assignments {
        let human_count = rooms.get(&room.game_room).copied().unwrap_or(0);
        let target = targets.target_for_room(&config, room.game_room, human_count);
        let mut existing = bot_snapshots
            .iter()
            .filter(|(_, _, _, bot_room)| *bot_room == room.game_room)
            .map(|(entity, id, snake, _)| (*entity, *id, *snake))
            .collect::<Vec<_>>();
        existing.sort_by_key(|(_, id, _)| id.to_bits());

        if existing.len() > target {
            for (player_entity, _, snake_entity) in existing.into_iter().skip(target) {
                if let Some(snake_entity) = snake_entity {
                    commands.entity(snake_entity).try_despawn();
                }
                commands.entity(player_entity).try_despawn();
            }
            continue;
        }

        for _ in existing.len()..target {
            let obstacle_tails = tails
                .iter()
                .filter(|(_, _, _, tail_room)| **tail_room == room.game_room)
                .map(|(head, tail, length, _)| visible_tail(head, tail, length))
                .collect::<Vec<_>>();
            spawn_bot(&mut commands, &config, *room, &mut ids, &obstacle_tails);
        }
    }

    let mut dead_bots = bot_queries.p1();
    for (player_entity, mut player, mut score, mut stats, mut status, room, respawn_ready_at) in
        &mut dead_bots
    {
        let room_bot_count = bot_snapshots
            .iter()
            .filter(|(_, _, _, bot_room)| bot_room == room)
            .count();
        let human_count = rooms.get(room).copied().unwrap_or(0);
        if room_bot_count > targets.target_for_room(&config, *room, human_count) {
            continue;
        }
        if player.snake.is_some() && *status == PlayerStatus::Alive {
            continue;
        }
        if respawn_ready_at.is_some_and(|ready_at| !ready_at.is_ready(time.elapsed_secs_f64())) {
            continue;
        }
        let Some(lightyear_room) = directory.lightyear_room(*room) else {
            continue;
        };
        let obstacle_tails = tails
            .iter()
            .filter(|(_, _, _, tail_room)| **tail_room == *room)
            .map(|(head, tail, length, _)| visible_tail(head, tail, length))
            .collect::<Vec<_>>();
        let snake = spawn_bot_snake(
            &mut commands,
            &config,
            *room,
            lightyear_room,
            player.id,
            &obstacle_tails,
        );
        commands.entity(snake).insert(HasPlayer(player_entity));
        commands.entity(player_entity).remove::<RespawnReadyAt>();
        player.snake = Some(snake);
        *score = PlayerScore::default();
        stats.reset_for_life();
        *status = PlayerStatus::Alive;
    }
}

pub(crate) fn clamp_bot_target(config: &GameConfig, count: usize, human_count: usize) -> usize {
    count.min(
        config
            .rooms
            .max_players_per_room
            .saturating_sub(human_count),
    )
}

fn spawn_bot<'a>(
    commands: &mut Commands,
    config: &GameConfig,
    assignment: RoomAssignment,
    ids: &mut BotIdAllocator,
    obstacle_tails: impl IntoIterator<Item = &'a TailPolyline>,
) {
    let bot_id = ids.next();
    let snake = spawn_bot_snake(
        commands,
        config,
        assignment.game_room,
        assignment.lightyear_room,
        bot_id,
        obstacle_tails,
    );
    let player = PlayerBundle::new_in_room(
        Player {
            id: bot_id,
            name: format!("Bot {}", bot_id.to_bits() - FIRST_BOT_ID),
            snake: Some(snake),
        },
        assignment.game_room,
    )
    .spawn(commands, bot_id);
    commands.entity(player).insert(BotMarker);
    commands.entity(snake).insert(HasPlayer(player));
    add_replicated_entity_to_room(commands, assignment.lightyear_room, player);
}

fn spawn_bot_snake<'a>(
    commands: &mut Commands,
    config: &GameConfig,
    room: RoomId,
    lightyear_room: LightyearRoomId,
    bot_id: PeerId,
    obstacle_tails: impl IntoIterator<Item = &'a TailPolyline>,
) -> Entity {
    let (spawn_position, spawn_direction) =
        snake_spawn_pose_avoiding(config, room, bot_id.to_bits(), obstacle_tails);
    let snake = SnakeBundle::spawn_server_owned_at(
        commands,
        bot_id.to_bits(),
        &config.movement,
        room,
        spawn_position,
        spawn_direction,
    );
    commands.entity(snake).insert((
        BotMarker,
        TurnRateLimiter::default(),
        BotController::new_with_mistakes(
            config.bots.decision_interval_ticks,
            bot_id.to_bits() ^ room.0.rotate_left(17),
            config.bots.mistake_chance_per_decision_percent,
        ),
    ));
    add_replicated_entity_to_room(commands, lightyear_room, snake);
    snake
}

fn drive_bots(
    mut commands: Commands,
    config: Res<GameConfig>,
    timeline: Res<LocalTimeline>,
    mut queries: ParamSet<(
        Query<(
            Entity,
            &SnakeHead,
            &TailPoints,
            Option<&TailLength>,
            &RoomId,
        )>,
        Query<
            (
                Entity,
                &RoomId,
                &mut SnakeHead,
                &TailPoints,
                &TailLength,
                &mut TurnRateLimiter,
                &mut BotController,
            ),
            With<BotMarker>,
        >,
    )>,
) {
    let tail_snapshots = queries
        .p0()
        .iter()
        .map(|(entity, head, tail, length, room)| (entity, *room, visible_tail(head, tail, length)))
        .collect::<Vec<_>>();

    let mut bot_query = queries.p1();
    let window_ticks = turn_rate_limit_window_ticks(&config);
    for (entity, room, mut head, tail, length, mut limiter, mut controller) in bot_query.iter_mut()
    {
        let obstacle_tails = tail_snapshots
            .iter()
            .filter(|(other_entity, other_room, _)| *other_entity != entity && other_room == room)
            .map(|(_, _, tail)| tail)
            .collect::<Vec<_>>();
        let visible_tail = visible_tail(&head, &tail, Some(length));
        let direction = controller.choose_direction_avoiding_limited(
            &visible_tail,
            &config.arena,
            &obstacle_tails,
            config.bots.max_turns_per_second,
            window_ticks,
        );
        turn_tail_with_diff_limited(
            &mut commands,
            entity,
            head.as_mut(),
            direction,
            limiter.as_mut(),
            timeline.tick().0,
            config.bots.max_turns_per_second,
            window_ticks,
        );
    }
}

fn visible_tail(head: &SnakeHead, tail: &TailPoints, length: Option<&TailLength>) -> TailPolyline {
    tail.polyline(
        head,
        length.map(|length| length.current_size).unwrap_or(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_bot_target_is_clamped_to_room_capacity() {
        let mut config = GameConfig::default();
        config.rooms.max_players_per_room = 8;

        assert_eq!(clamp_bot_target(&config, 7, 0), 7);
        assert_eq!(clamp_bot_target(&config, 80, 0), 8);
        assert_eq!(clamp_bot_target(&config, 7, 3), 5);
        assert_eq!(clamp_bot_target(&config, 7, 8), 0);
    }

    #[test]
    fn override_target_is_used_even_when_default_bots_are_disabled() {
        let mut config = GameConfig::default();
        config.bots.enabled = false;
        config.bots.target_count_per_room = 4;
        let mut targets = BotTargetOverrides::default();
        let room = RoomId(2);

        assert_eq!(targets.target_for_room(&config, room, 0), 0);
        assert_eq!(targets.set_target(&config, room, 3), 3);
        assert_eq!(targets.target_for_room(&config, room, 0), 0);
        assert_eq!(targets.target_for_room(&config, room, 1), 3);
    }

    #[test]
    fn default_bot_target_fills_room_to_minimum_total_players() {
        let mut config = GameConfig::default();
        config.rooms.max_players_per_room = 8;
        config.bots.enabled = true;
        config.bots.target_count_per_room = 0;
        config.bots.minimum_total_players_per_room = 5;
        let targets = BotTargetOverrides::default();
        let room = RoomId(2);

        assert_eq!(targets.target_for_room(&config, room, 0), 0);
        assert_eq!(targets.target_for_room(&config, room, 1), 4);
        assert_eq!(targets.target_for_room(&config, room, 4), 1);
        assert_eq!(targets.target_for_room(&config, room, 5), 0);
        assert_eq!(targets.target_for_room(&config, room, 8), 0);
    }

    #[test]
    fn configured_bot_target_is_a_floor_over_dynamic_fill() {
        let mut config = GameConfig::default();
        config.rooms.max_players_per_room = 8;
        config.bots.enabled = true;
        config.bots.target_count_per_room = 2;
        config.bots.minimum_total_players_per_room = 5;
        let targets = BotTargetOverrides::default();
        let room = RoomId(2);

        assert_eq!(targets.target_for_room(&config, room, 0), 0);
        assert_eq!(targets.target_for_room(&config, room, 3), 2);
        assert_eq!(targets.target_for_room(&config, room, 5), 2);
        assert_eq!(targets.target_for_room(&config, room, 7), 1);
    }
}
