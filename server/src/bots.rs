use bevy::prelude::*;
use lightyear::prelude::PeerId;

use crate::rooms::{add_replicated_entity_to_room, RoomAssignment, RoomDirectory};
use crate::spawning::snake_spawn_pose;
use shared::bot::{BotController, BotMarker};
use shared::config::GameConfig;
use shared::movement::{turn_tail, SimulationSet};
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

pub(crate) struct ServerBotsPlugin;

impl Plugin for ServerBotsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BotIdAllocator>();
        app.add_systems(Update, maintain_bots);
        app.add_systems(FixedUpdate, drive_bots.before(SimulationSet::Movement));
    }
}

fn maintain_bots(
    mut commands: Commands,
    config: Res<GameConfig>,
    directory: Res<RoomDirectory>,
    mut ids: ResMut<BotIdAllocator>,
    bot_players: Query<&RoomId, (With<Player>, With<BotMarker>)>,
    mut dead_bots: Query<
        (
            Entity,
            &mut Player,
            &mut PlayerScore,
            &mut PlayerStatus,
            &RoomId,
        ),
        With<BotMarker>,
    >,
) {
    if !config.bots.enabled {
        return;
    }

    let rooms = directory.iter().collect::<Vec<_>>();
    for room in &rooms {
        let existing = bot_players
            .iter()
            .filter(|bot_room| **bot_room == room.game_room)
            .count();
        for _ in existing..config.bots.target_count_per_room {
            spawn_bot(&mut commands, &config, *room, &mut ids);
        }
    }

    for (player_entity, mut player, mut score, mut status, room) in &mut dead_bots {
        if player.snake.is_some() && *status == PlayerStatus::Alive {
            continue;
        }
        let Some(lightyear_room) = directory.lightyear_room(*room) else {
            continue;
        };
        let snake = spawn_bot_snake(&mut commands, &config, *room, lightyear_room, player.id);
        commands.entity(snake).insert(HasPlayer(player_entity));
        player.snake = Some(snake);
        *score = PlayerScore::from_length(config.movement.starting_tail_length);
        *status = PlayerStatus::Alive;
    }
}

fn spawn_bot(
    commands: &mut Commands,
    config: &GameConfig,
    assignment: RoomAssignment,
    ids: &mut BotIdAllocator,
) {
    let bot_id = ids.next();
    let snake = spawn_bot_snake(
        commands,
        config,
        assignment.game_room,
        assignment.lightyear_room,
        bot_id,
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
    commands.entity(player).insert((
        BotMarker,
        PlayerScore::from_length(config.movement.starting_tail_length),
    ));
    commands.entity(snake).insert(HasPlayer(player));
    add_replicated_entity_to_room(commands, assignment.lightyear_room, player);
}

fn spawn_bot_snake(
    commands: &mut Commands,
    config: &GameConfig,
    room: RoomId,
    lightyear_room: Entity,
    bot_id: PeerId,
) -> Entity {
    let (spawn_position, spawn_direction) = snake_spawn_pose(config, room, bot_id.to_bits());
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
        BotController::new(
            config.bots.decision_interval_ticks,
            bot_id.to_bits() ^ room.0.rotate_left(17),
        ),
    ));
    add_replicated_entity_to_room(commands, lightyear_room, snake);
    snake
}

fn drive_bots(
    config: Res<GameConfig>,
    mut queries: ParamSet<(
        Query<(Entity, &TailPoints, &RoomId)>,
        Query<(Entity, &RoomId, &mut TailPoints, &mut BotController), With<BotMarker>>,
    )>,
) {
    let tail_snapshots = queries
        .p0()
        .iter()
        .map(|(entity, tail, room)| (entity, *room, tail.clone()))
        .collect::<Vec<_>>();

    let mut bot_query = queries.p1();
    for (entity, room, mut tail, mut controller) in bot_query.iter_mut() {
        let obstacle_tails = tail_snapshots
            .iter()
            .filter(|(other_entity, other_room, _)| *other_entity != entity && other_room == room)
            .map(|(_, _, tail)| tail)
            .collect::<Vec<_>>();
        let direction = controller.choose_direction_avoiding(&tail, &config.arena, &obstacle_tails);
        turn_tail(&mut tail, direction);
    }
}
