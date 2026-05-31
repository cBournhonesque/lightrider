use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    MessageReceiver, NetworkTarget, PeerId, RemoteId, Server, ServerMultiMessageSender,
};
use tracing::{error, warn};

use crate::bots::BotTargetOverrides;
use crate::rooms::ClientRoom;
use shared::bot::BotMarker;
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

const ADMIN_PASSWORD_ENV: &str = "LIGHTRIDER_ADMIN_PASSWORD";

#[derive(Component, Clone, Copy, Debug, Default)]
struct AdminClient;

#[derive(Resource, Clone, Debug, Default)]
struct AdminAuth {
    password: Option<String>,
}

impl AdminAuth {
    fn from_env() -> Self {
        let password = std::env::var(ADMIN_PASSWORD_ENV)
            .ok()
            .map(|password| password.trim().to_string())
            .filter(|password| !password.is_empty());
        if password.is_none() {
            warn!(
                "{ADMIN_PASSWORD_ENV} is not set; admin login is disabled for this server process"
            );
        }
        Self { password }
    }

    fn accepts(&self, password: &str) -> bool {
        self.password.as_deref().is_some_and(|expected| {
            !expected.is_empty() && expected.as_bytes() == password.as_bytes()
        })
    }
}

pub(crate) struct ServerAdminPlugin;

impl Plugin for ServerAdminPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AdminAuth::from_env());
        app.add_systems(Update, (handle_admin_logins, handle_admin_commands));
    }
}

fn handle_admin_logins(
    auth: Res<AdminAuth>,
    server: Single<&Server>,
    config: Res<GameConfig>,
    targets: Res<BotTargetOverrides>,
    bot_players: Query<&RoomId, (With<Player>, With<BotMarker>)>,
    mut sender: ServerMultiMessageSender,
    mut clients: Query<
        (
            Entity,
            &RemoteId,
            Option<&ClientRoom>,
            &mut MessageReceiver<AdminLoginRequest>,
        ),
        (With<ClientOf>, With<Connected>),
    >,
    mut commands: Commands,
) {
    let server = server.into_inner();
    for (client_entity, remote_id, client_room, mut receiver) in &mut clients {
        for request in receiver.receive() {
            let accepted = auth.accepts(request.password.trim());
            if accepted {
                commands.entity(client_entity).insert(AdminClient);
            }
            let room = client_room.map(|client_room| client_room.room);
            let response = AdminResponse {
                accepted,
                message: if accepted {
                    "admin unlocked".to_string()
                } else if auth.password.is_some() {
                    "admin password rejected".to_string()
                } else {
                    "admin disabled on this server".to_string()
                },
                status: admin_status(room, &config, &targets, &bot_players, accepted),
            };
            send_admin_response(&mut sender, server, remote_id.0, response);
        }
    }
}

fn handle_admin_commands(
    server: Single<&Server>,
    config: Res<GameConfig>,
    mut targets: ResMut<BotTargetOverrides>,
    bot_players: Query<&RoomId, (With<Player>, With<BotMarker>)>,
    mut sender: ServerMultiMessageSender,
    mut clients: Query<
        (
            &RemoteId,
            Option<&ClientRoom>,
            &mut MessageReceiver<AdminCommand>,
            Has<AdminClient>,
        ),
        (With<ClientOf>, With<Connected>),
    >,
) {
    let server = server.into_inner();
    for (remote_id, client_room, mut receiver, is_admin) in &mut clients {
        for command in receiver.receive() {
            if !is_admin {
                send_admin_response(
                    &mut sender,
                    server,
                    remote_id.0,
                    AdminResponse {
                        accepted: false,
                        message: "admin command rejected".to_string(),
                        status: admin_status(
                            client_room.map(|client_room| client_room.room),
                            &config,
                            &targets,
                            &bot_players,
                            false,
                        ),
                    },
                );
                continue;
            }

            let Some(room) = client_room.map(|client_room| client_room.room) else {
                send_admin_response(
                    &mut sender,
                    server,
                    remote_id.0,
                    AdminResponse {
                        accepted: false,
                        message: "admin command needs an assigned room".to_string(),
                        status: admin_status(None, &config, &targets, &bot_players, true),
                    },
                );
                continue;
            };

            let message = match command {
                AdminCommand::SetNumBots { count } => {
                    let count = targets.set_target(&config, room, usize::from(count));
                    format!("bot target set to {count}")
                }
            };
            send_admin_response(
                &mut sender,
                server,
                remote_id.0,
                AdminResponse {
                    accepted: true,
                    message,
                    status: admin_status(Some(room), &config, &targets, &bot_players, true),
                },
            );
        }
    }
}

fn admin_status(
    room: Option<RoomId>,
    config: &GameConfig,
    targets: &BotTargetOverrides,
    bot_players: &Query<&RoomId, (With<Player>, With<BotMarker>)>,
    authenticated: bool,
) -> AdminStatus {
    let Some(room) = room else {
        return AdminStatus {
            authenticated,
            ..default()
        };
    };
    AdminStatus {
        authenticated,
        room: Some(room),
        bot_count: saturating_u16(
            bot_players
                .iter()
                .filter(|bot_room| **bot_room == room)
                .count(),
        ),
        target_bot_count: saturating_u16(targets.target_for_room(config, room)),
    }
}

fn send_admin_response(
    sender: &mut ServerMultiMessageSender,
    server: &Server,
    peer_id: PeerId,
    response: AdminResponse,
) {
    if let Err(error) =
        sender.send::<_, GameChannel>(&response, server, &NetworkTarget::Single(peer_id))
    {
        error!(?error, ?peer_id, "failed to send admin response");
    }
}

fn saturating_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_auth_rejects_when_env_password_is_absent() {
        let auth = AdminAuth { password: None };

        assert!(!auth.accepts("anything"));
    }

    #[test]
    fn admin_auth_compares_password_bytes() {
        let auth = AdminAuth {
            password: Some("secret".to_string()),
        };

        assert!(auth.accepts("secret"));
        assert!(!auth.accepts("Secret"));
        assert!(!auth.accepts("secret "));
    }
}
