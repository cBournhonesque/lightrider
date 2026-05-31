use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::{Client, InputTimeline, IsSynced, MessageSender};

use shared::network::protocol::prelude::{
    GameChannel, PlayerNameUpdate, RoomCode, RoomId, RoomJoinMode, RoomJoinRequest,
};

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
struct RoomJoinSettings {
    mode: RoomJoinMode,
    name: String,
}

pub(crate) struct ClientRoomsPlugin {
    pub(crate) mode: RoomJoinMode,
    pub(crate) name: String,
}

impl Plugin for ClientRoomsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RoomJoinSettings {
            mode: self.mode,
            name: sanitize_player_name(&self.name),
        });
        app.add_systems(Update, (send_player_name_update, send_room_join_request));
    }
}

pub(crate) fn parse_room_join_mode(value: &str) -> Result<RoomJoinMode, String> {
    match value.to_ascii_lowercase().as_str() {
        "auto" | "random" => Ok(RoomJoinMode::Auto),
        "new" | "create" => Ok(RoomJoinMode::New),
        _ => RoomCode::parse(value)
            .map(RoomJoinMode::Private)
            .or_else(|_| {
                value
                    .parse::<u64>()
                    .map(|id| RoomJoinMode::Specific(RoomId(id)))
            })
            .map_err(|_| {
                "expected `auto`, `new`, a four-letter private code, or a numeric room id"
                    .to_string()
            }),
    }
}

fn send_player_name_update(
    settings: Res<RoomJoinSettings>,
    mut sent: Local<bool>,
    mut clients: Query<
        &mut MessageSender<PlayerNameUpdate>,
        (With<Client>, With<Connected>, With<IsSynced<InputTimeline>>),
    >,
) {
    if *sent {
        return;
    }
    let Ok(mut sender) = clients.single_mut() else {
        return;
    };
    sender.send::<GameChannel>(PlayerNameUpdate {
        name: settings.name.clone(),
    });
    *sent = true;
}

fn send_room_join_request(
    settings: Res<RoomJoinSettings>,
    mut sent: Local<bool>,
    mut clients: Query<
        &mut MessageSender<RoomJoinRequest>,
        (With<Client>, With<Connected>, With<IsSynced<InputTimeline>>),
    >,
) {
    if *sent {
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

pub(crate) fn sanitize_player_name(name: &str) -> String {
    let trimmed = name.trim();
    let sanitized = if trimmed.is_empty() {
        "Player"
    } else {
        trimmed
    };
    sanitized.chars().take(18).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_room_join_modes() {
        assert_eq!(parse_room_join_mode("auto"), Ok(RoomJoinMode::Auto));
        assert_eq!(parse_room_join_mode("new"), Ok(RoomJoinMode::New));
        assert_eq!(
            parse_room_join_mode("abcd"),
            Ok(RoomJoinMode::Private(RoomCode::parse("ABCD").unwrap()))
        );
        assert_eq!(
            parse_room_join_mode("42"),
            Ok(RoomJoinMode::Specific(RoomId(42)))
        );
        assert!(parse_room_join_mode("bogus").is_err());
    }

    #[test]
    fn sanitizes_player_names() {
        assert_eq!(sanitize_player_name("  Ada  "), "Ada");
        assert_eq!(sanitize_player_name(""), "Player");
        assert_eq!(
            sanitize_player_name("12345678901234567890"),
            "123456789012345678"
        );
    }
}
