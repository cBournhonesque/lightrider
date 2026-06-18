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

#[derive(Resource, Default, Clone, Debug, PartialEq, Eq)]
struct LastSentRoomJoinSettings {
    mode: Option<RoomJoinMode>,
    name: Option<String>,
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
        app.init_resource::<LastSentRoomJoinSettings>();
        #[cfg(all(target_family = "wasm", feature = "lightyear-matchmaker"))]
        app.add_systems(
            Update,
            (
                read_browser_room_join_settings,
                send_room_join_request,
                send_player_name_update,
            )
                .chain(),
        );
        #[cfg(not(all(target_family = "wasm", feature = "lightyear-matchmaker")))]
        app.add_systems(
            Update,
            (send_room_join_request, send_player_name_update).chain(),
        );
    }
}

pub(crate) fn parse_room_join_mode(value: &str) -> Result<RoomJoinMode, String> {
    match value.to_ascii_lowercase().as_str() {
        "" | "auto" | "random" => Ok(RoomJoinMode::Auto),
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
    mut last_sent: ResMut<LastSentRoomJoinSettings>,
    mut clients: Query<
        &mut MessageSender<PlayerNameUpdate>,
        (With<Client>, With<Connected>, With<IsSynced<InputTimeline>>),
    >,
) {
    if settings.name.is_empty() || last_sent.name.as_deref() == Some(settings.name.as_str()) {
        return;
    }
    let Ok(mut sender) = clients.single_mut() else {
        return;
    };
    sender.send::<GameChannel>(PlayerNameUpdate {
        name: settings.name.clone(),
    });
    last_sent.name = Some(settings.name.clone());
}

fn send_room_join_request(
    settings: Res<RoomJoinSettings>,
    mut last_sent: ResMut<LastSentRoomJoinSettings>,
    mut clients: Query<
        &mut MessageSender<RoomJoinRequest>,
        (With<Client>, With<Connected>, With<IsSynced<InputTimeline>>),
    >,
) {
    if last_sent.mode == Some(settings.mode) {
        return;
    }
    let Ok(mut sender) = clients.single_mut() else {
        return;
    };
    sender.send::<GameChannel>(RoomJoinRequest {
        mode: settings.mode,
    });
    last_sent.mode = Some(settings.mode);
}

pub(crate) fn sanitize_player_name(name: &str) -> String {
    name.trim().chars().take(18).collect()
}

#[cfg(all(target_family = "wasm", feature = "lightyear-matchmaker"))]
fn read_browser_room_join_settings(mut settings: ResMut<RoomJoinSettings>) {
    let Some((name, room)) = browser_room_join_settings() else {
        return;
    };
    let mode = parse_room_join_mode(&room).unwrap_or(RoomJoinMode::Auto);
    let name = sanitize_player_name(&name);
    if settings.name != name || settings.mode != mode {
        settings.name = name;
        settings.mode = mode;
    }
}

#[cfg(all(target_family = "wasm", feature = "lightyear-matchmaker"))]
fn browser_room_join_settings() -> Option<(String, String)> {
    let window = web_sys::window()?;
    let object = js_sys::Reflect::get(
        window.as_ref(),
        &wasm_bindgen::JsValue::from_str("LIGHTRIDER_PLAYER_SETTINGS"),
    )
    .ok()?;
    if object.is_null() || object.is_undefined() {
        return None;
    }
    let name = browser_setting_string(&object, "name").unwrap_or_default();
    let room = browser_setting_string(&object, "room").unwrap_or_default();
    Some((name, room))
}

#[cfg(all(target_family = "wasm", feature = "lightyear-matchmaker"))]
fn browser_setting_string(object: &wasm_bindgen::JsValue, key: &str) -> Option<String> {
    js_sys::Reflect::get(object, &wasm_bindgen::JsValue::from_str(key))
        .ok()
        .and_then(|value| value.as_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_room_join_modes() {
        assert_eq!(parse_room_join_mode("auto"), Ok(RoomJoinMode::Auto));
        assert_eq!(parse_room_join_mode(""), Ok(RoomJoinMode::Auto));
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
        assert_eq!(sanitize_player_name(""), "");
        assert_eq!(
            sanitize_player_name("12345678901234567890"),
            "123456789012345678"
        );
    }
}
