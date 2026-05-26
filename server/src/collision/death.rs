use crate::respawn::{respawn_delay_seconds, RespawnReadyAt};
use crate::rooms::{remove_replicated_entity_from_room, RoomDirectory};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use lightyear::prelude::{ControlledBy, NetworkTarget, Server, ServerMultiMessageSender};
use shared::bot::BotMarker;
use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;
use tracing::error;

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
    human_players: Query<(), With<ControlledBy>>,
    snakes: Query<(&HasPlayer, &RoomId)>,
    mut commands: Commands,
) {
    let server = server.into_inner();
    let mut killed_snakes = EntityHashSet::default();
    for collision_event in reader.read() {
        if !killed_snakes.insert(collision_event.killed) {
            continue;
        }
        let Ok((killed_player, killed_room)) = snakes.get(collision_event.killed) else {
            error!("snake does not have HasPlayer component");
            continue;
        };
        let Ok((killer_player, killer_room)) = snakes.get(collision_event.killer) else {
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
        info!(?collision_event, "Collision event!");

        let involves_human =
            human_players.contains(killed_player.0) || human_players.contains(killer_player.0);
        if involves_human {
            // We only notify clients for human-involved deaths for now. Room-scoped
            // replicated despawns are enough for bot-only churn, and this avoids sending
            // mapped entity messages before a late-joining client has seen those bot entities.
            let _ = sender
                .send::<_, GameChannel>(
                    &PlayerDeath {
                        killer_player: killer_player.0,
                        killed_player: killed_player.0,
                        killer_snake: collision_event.killer,
                        killed_snake: collision_event.killed,
                        killer_name,
                        killed_name,
                        room: *killed_room,
                        reason: collision_event.reason,
                        stats: killed_stats,
                    },
                    server,
                    &NetworkTarget::All,
                )
                .map_err(|e| error!(?e, "Failed to send message"));
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
