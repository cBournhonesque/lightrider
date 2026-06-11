use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{NetworkTarget, RemoteId, Server, ServerMultiMessageSender};
use tracing::error;

use crate::rooms::ClientRoom;
use shared::network::protocol::prelude::*;

const LEADERBOARD_SNAPSHOT_SECONDS: f32 = 0.5;

pub(crate) struct ServerLeaderboardPlugin;

#[derive(Resource, Default)]
struct LeaderboardSnapshotSequence(u32);

#[derive(Clone, Debug)]
struct LeaderboardRow {
    player: Entity,
    room: RoomId,
    name: String,
    score: u32,
    status: PlayerStatus,
    stats: PlayerStats,
}

impl Plugin for ServerLeaderboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LeaderboardSnapshotSequence>();
        app.add_systems(Update, send_leaderboard_snapshots);
    }
}

fn send_leaderboard_snapshots(
    time: Res<Time>,
    servers: Query<&Server>,
    mut sequence: ResMut<LeaderboardSnapshotSequence>,
    mut timer: Local<Option<Timer>>,
    mut sender: ServerMultiMessageSender,
    clients: Query<(&RemoteId, &ClientRoom), (With<ClientOf>, With<Connected>)>,
    players: Query<(
        Entity,
        &Player,
        &PlayerScore,
        &PlayerStats,
        &PlayerStatus,
        &RoomId,
    )>,
) {
    let timer = timer.get_or_insert_with(|| {
        Timer::from_seconds(LEADERBOARD_SNAPSHOT_SECONDS, TimerMode::Repeating)
    });
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }

    let Some(server) = servers.iter().next() else {
        return;
    };
    sequence.0 = sequence.0.wrapping_add(1);
    let sequence = sequence.0;
    let entries_by_room = leaderboard_entries_by_room(players.iter().map(
        |(entity, player, score, stats, status, room)| LeaderboardRow {
            player: entity,
            room: *room,
            name: player.name.clone(),
            score: score.value,
            status: *status,
            stats: *stats,
        },
    ));

    for (remote_id, client_room) in &clients {
        let snapshot = LeaderboardSnapshot {
            sequence,
            room: client_room.room,
            entries: entries_by_room
                .get(&client_room.room)
                .cloned()
                .unwrap_or_default(),
        };
        if let Err(error) = sender.send::<_, LeaderboardChannel>(
            &snapshot,
            server,
            &NetworkTarget::Single(remote_id.0),
        ) {
            error!(
                ?error,
                peer_id = ?remote_id.0,
                room = client_room.room.0,
                "failed to send leaderboard snapshot"
            );
        }
    }
}

fn leaderboard_entries_by_room(
    rows: impl IntoIterator<Item = LeaderboardRow>,
) -> HashMap<RoomId, Vec<LeaderboardEntrySnapshot>> {
    let mut rows = rows.into_iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.room
            .0
            .cmp(&right.room.0)
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.player.to_bits().cmp(&right.player.to_bits()))
    });

    let mut entries = HashMap::new();
    let mut current_room = None;
    let mut rank = 0_u16;
    for row in rows {
        if current_room != Some(row.room) {
            current_room = Some(row.room);
            rank = 1;
        } else {
            rank = rank.saturating_add(1);
        }
        entries
            .entry(row.room)
            .or_insert_with(Vec::new)
            .push(LeaderboardEntrySnapshot {
                player: row.player,
                name: row.name,
                score: row.score,
                rank,
                status: row.status,
                stats: row.stats,
            });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaderboard_entries_are_ranked_per_room() {
        let room_a = RoomId(1);
        let room_b = RoomId(2);
        let rows = [
            row(11, room_a, 10),
            row(12, room_a, 30),
            row(13, room_b, 20),
            row(14, room_a, 30),
        ];

        let entries = leaderboard_entries_by_room(rows);
        let room_a_entries = entries.get(&room_a).unwrap();
        let room_b_entries = entries.get(&room_b).unwrap();

        assert_eq!(room_a_entries[0].player, Entity::from_bits(12));
        assert_eq!(room_a_entries[0].rank, 1);
        assert_eq!(room_a_entries[1].player, Entity::from_bits(14));
        assert_eq!(room_a_entries[1].rank, 2);
        assert_eq!(room_a_entries[2].player, Entity::from_bits(11));
        assert_eq!(room_a_entries[2].rank, 3);
        assert_eq!(room_b_entries[0].player, Entity::from_bits(13));
        assert_eq!(room_b_entries[0].rank, 1);
    }

    fn row(bits: u64, room: RoomId, score: u32) -> LeaderboardRow {
        LeaderboardRow {
            player: Entity::from_bits(bits),
            room,
            name: format!("Player {bits}"),
            score,
            status: PlayerStatus::Alive,
            stats: PlayerStats::default(),
        }
    }
}
