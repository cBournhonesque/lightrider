use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::{Client, MessageSender};

use shared::network::protocol::prelude::{GameChannel, RoomId, RoomJoinMode, RoomJoinRequest};

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
struct RoomJoinSettings {
    mode: RoomJoinMode,
}

pub(crate) struct ClientRoomsPlugin {
    pub(crate) mode: RoomJoinMode,
}

impl Plugin for ClientRoomsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RoomJoinSettings { mode: self.mode });
        app.add_systems(Update, send_room_join_request);
    }
}

pub(crate) fn parse_room_join_mode(value: &str) -> Result<RoomJoinMode, String> {
    match value.to_ascii_lowercase().as_str() {
        "auto" | "random" => Ok(RoomJoinMode::Auto),
        "new" | "create" => Ok(RoomJoinMode::New),
        _ => value
            .parse::<u64>()
            .map(|id| RoomJoinMode::Specific(RoomId(id)))
            .map_err(|_| "expected `auto`, `new`, or a numeric room id".to_string()),
    }
}

fn send_room_join_request(
    settings: Res<RoomJoinSettings>,
    mut sent: Local<bool>,
    mut clients: Query<&mut MessageSender<RoomJoinRequest>, (With<Client>, With<Connected>)>,
) {
    if *sent || settings.mode == RoomJoinMode::Auto {
        return;
    }
    let Ok(mut sender) = clients.single_mut() else {
        return;
    };
    sender.send::<GameChannel>(RoomJoinRequest {
        mode: settings.mode,
    });
    *sent = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_room_join_modes() {
        assert_eq!(parse_room_join_mode("auto"), Ok(RoomJoinMode::Auto));
        assert_eq!(parse_room_join_mode("new"), Ok(RoomJoinMode::New));
        assert_eq!(
            parse_room_join_mode("42"),
            Ok(RoomJoinMode::Specific(RoomId(42)))
        );
        assert!(parse_room_join_mode("bogus").is_err());
    }
}
