use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryWindow, Window};
use lightyear::connection::client::Connected;
use lightyear::prelude::{Client, InputTimeline, IsSynced, MessageSender};

use shared::config::GameConfig;
use shared::network::protocol::prelude::{ClientViewportUpdate, GameChannel};

pub(crate) struct ClientViewportPlugin;

impl Plugin for ClientViewportPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, send_client_viewport_updates);
    }
}

fn send_client_viewport_updates(
    config: Res<GameConfig>,
    monitors: Query<&Monitor>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sent: Local<Option<(Entity, ClientViewportUpdate)>>,
    mut clients: Query<
        (Entity, &mut MessageSender<ClientViewportUpdate>),
        (With<Client>, With<Connected>, With<IsSynced<InputTimeline>>),
    >,
) {
    let Ok((client, mut sender)) = clients.single_mut() else {
        return;
    };
    let size = detect_max_screen_size(&config, &monitors, &windows);
    let update = ClientViewportUpdate {
        max_screen_width: size.x,
        max_screen_height: size.y,
    };
    if sent.is_some_and(|(sent_client, sent_update)| sent_client == client && sent_update == update)
    {
        return;
    }
    sender.send::<GameChannel>(update);
    *sent = Some((client, update));
}

fn detect_max_screen_size(
    config: &GameConfig,
    monitors: &Query<&Monitor>,
    windows: &Query<&Window, With<PrimaryWindow>>,
) -> UVec2 {
    let detected = monitors
        .iter()
        .map(Monitor::physical_size)
        .chain(windows.single().ok().map(Window::physical_size))
        .filter(|size| size.x > 0 && size.y > 0)
        .max_by_key(|size| u64::from(size.x) * u64::from(size.y))
        .unwrap_or_else(|| config.network.interest.fallback_screen_size());
    config
        .network
        .interest
        .clamp_screen_size(detected.x, detected.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_message_is_clamped() {
        let mut config = GameConfig::default();
        config.network.interest.max_screen_width = 1600;
        config.network.interest.max_screen_height = 900;

        let size = config.network.interest.clamp_screen_size(8000, 4000);

        assert_eq!(size, UVec2::new(1600, 900));
    }
}
