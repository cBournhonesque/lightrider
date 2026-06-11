use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::{Client, MessageReceiver};

use shared::network::protocol::prelude::LeaderboardSnapshot;

pub(crate) struct ClientLeaderboardPlugin;

#[derive(Resource, Default, Clone, Debug)]
pub(crate) struct LeaderboardState {
    latest: Option<LeaderboardSnapshot>,
}

impl LeaderboardState {
    pub(crate) fn latest(&self) -> Option<&LeaderboardSnapshot> {
        self.latest.as_ref()
    }

    fn accept(&mut self, snapshot: LeaderboardSnapshot) {
        let should_accept = self.latest.as_ref().is_none_or(|latest| {
            latest.room != snapshot.room || latest.sequence < snapshot.sequence
        });
        if should_accept {
            self.latest = Some(snapshot);
        }
    }
}

impl Plugin for ClientLeaderboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LeaderboardState>();
        app.add_systems(Update, receive_leaderboard_snapshots);
    }
}

fn receive_leaderboard_snapshots(
    mut state: ResMut<LeaderboardState>,
    mut clients: Query<&mut MessageReceiver<LeaderboardSnapshot>, (With<Client>, With<Connected>)>,
) {
    let Ok(mut receiver) = clients.single_mut() else {
        return;
    };
    for snapshot in receiver.receive() {
        state.accept(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::network::protocol::prelude::RoomId;

    #[test]
    fn state_keeps_newest_snapshot_for_room() {
        let mut state = LeaderboardState::default();

        state.accept(snapshot(2, RoomId(1)));
        state.accept(snapshot(1, RoomId(1)));

        assert_eq!(state.latest().unwrap().sequence, 2);
    }

    #[test]
    fn state_accepts_room_changes() {
        let mut state = LeaderboardState::default();

        state.accept(snapshot(2, RoomId(1)));
        state.accept(snapshot(1, RoomId(2)));

        assert_eq!(state.latest().unwrap().room, RoomId(2));
        assert_eq!(state.latest().unwrap().sequence, 1);
    }

    fn snapshot(sequence: u32, room: RoomId) -> LeaderboardSnapshot {
        LeaderboardSnapshot {
            sequence,
            room,
            entries: Vec::new(),
        }
    }
}
