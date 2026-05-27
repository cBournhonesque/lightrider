use bevy::app::{App, Plugin};
use bevy::prelude::*;
use lightyear::prelude::input::bei::Fire;
use lightyear::prelude::ControlledBy;
use tracing::info;

use crate::respawn::RespawnReadyAt;
use crate::rooms::{add_replicated_entity_to_room, RoomDirectory};
use crate::spawning::snake_spawn_pose_avoiding;
use shared::config::GameConfig;
use shared::network::bundle::snake::SnakeBundle;
use shared::network::protocol::prelude::*;

pub struct NetworkInputsPlugin;

impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(handle_spawn_action);
    }
}

pub(crate) fn handle_spawn_action(
    trigger: On<Fire<SpawnPlayer>>,
    mut commands: Commands,
    config: Res<GameConfig>,
    directory: Res<RoomDirectory>,
    time: Res<Time>,
    mut players: Query<(
        &mut Player,
        &mut PlayerScore,
        &mut PlayerStats,
        &mut PlayerStatus,
        &RoomId,
        Option<&ControlledBy>,
        Option<&RespawnReadyAt>,
    )>,
    tails: Query<(&TailPoints, &RoomId)>,
) {
    let player_entity = trigger.context;
    let Ok((mut player, mut score, mut stats, mut status, room, controlled_by, respawn_ready_at)) =
        players.get_mut(player_entity)
    else {
        return;
    };
    if player.snake.is_some() {
        return;
    }
    if respawn_ready_at.is_some_and(|ready_at| !ready_at.is_ready(time.elapsed_secs_f64())) {
        return;
    }

    info!(?player, "Respawning player");
    let client_id = player.id;
    let obstacle_tails = tails
        .iter()
        .filter(|(_, tail_room)| **tail_room == *room)
        .map(|(tail, _)| tail);
    let (spawn_position, spawn_direction) =
        snake_spawn_pose_avoiding(&config, *room, client_id.to_bits(), obstacle_tails);
    let head_entity = SnakeBundle::spawn_with_room_at(
        &mut commands,
        client_id,
        &config.movement,
        *room,
        spawn_position,
        spawn_direction,
    );
    commands
        .entity(head_entity)
        .insert(HasPlayer(player_entity));
    if let Some(controlled_by) = controlled_by.copied() {
        commands.entity(head_entity).insert(controlled_by);
    }
    if let Some(lightyear_room) = directory.lightyear_room(*room) {
        add_replicated_entity_to_room(&mut commands, lightyear_room, head_entity);
    }
    spawn_snake_input_actions(&mut commands, head_entity, client_id, true);
    commands.entity(player_entity).remove::<RespawnReadyAt>();
    player.snake = Some(head_entity);
    *score = PlayerScore::default();
    stats.reset_for_life();
    *status = PlayerStatus::Alive;
}
