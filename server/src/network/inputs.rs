use bevy::app::{App, Plugin};
use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::{ControlledBy, MessageReceiver, RemoteId};
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
        app.add_systems(Update, handle_spawn_requests);
    }
}

pub(crate) fn handle_spawn_requests(
    mut commands: Commands,
    config: Res<GameConfig>,
    directory: Res<RoomDirectory>,
    time: Res<Time>,
    mut clients: Query<(&RemoteId, &mut MessageReceiver<PlayerSpawnRequest>), With<Connected>>,
    mut players: Query<(
        Entity,
        &mut Player,
        &mut PlayerScore,
        &mut PlayerStats,
        &mut PlayerStatus,
        &RoomId,
        Option<&ControlledBy>,
        Option<&RespawnReadyAt>,
    )>,
    tails: Query<(&TailPoints, Option<&TailLength>, &RoomId)>,
) {
    for (remote_id, mut receiver) in &mut clients {
        for _request in receiver.receive() {
            let Some((
                player_entity,
                mut player,
                mut score,
                mut stats,
                mut status,
                room,
                controlled_by,
                respawn_ready_at,
            )) = players
                .iter_mut()
                .find(|(_, player, ..)| player.id == remote_id.0)
            else {
                continue;
            };
            if player.snake.is_some() {
                continue;
            }
            if respawn_ready_at.is_some_and(|ready_at| !ready_at.is_ready(time.elapsed_secs_f64()))
            {
                continue;
            }

            info!(?player, "Respawning player");
            let client_id = player.id;
            let obstacle_tails = tails
                .iter()
                .filter(|(_, _, tail_room)| **tail_room == *room)
                .map(|(tail, length, _)| visible_tail(tail, length))
                .collect::<Vec<_>>();
            let (spawn_position, spawn_direction) =
                snake_spawn_pose_avoiding(&config, *room, client_id.to_bits(), &obstacle_tails);
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
    }
}

fn visible_tail(tail: &TailPoints, length: Option<&TailLength>) -> TailPoints {
    length
        .map(|length| tail.clipped_to_length(length.current_size))
        .unwrap_or_else(|| tail.clone())
}
