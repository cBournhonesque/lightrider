use bevy::prelude::*;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeClient;
use lightyear::prelude::client::{
    ClientPlugins, InputDelayConfig as LightyearInputDelayConfig, InputTimelineConfig,
    WebTransportClientIo,
};
use lightyear::prelude::*;
use std::net::{Ipv4Addr, SocketAddr};

use shared::config::GameConfig;
use shared::network::config::NetcodeIdentity;

#[derive(Resource, Clone)]
pub(crate) struct ClientConnectionConfig {
    pub(crate) client_port: u16,
    pub(crate) mode: ClientConnectionMode,
}

#[derive(Clone)]
pub(crate) enum ClientConnectionMode {
    Direct {
        client_id: u64,
        server_addr: SocketAddr,
        cert_digest: String,
    },
    #[cfg(feature = "bevygap")]
    Bevygap,
}

impl ClientConnectionConfig {
    pub(crate) fn direct(
        client_id: u64,
        client_port: u16,
        server_addr: SocketAddr,
        cert_digest: String,
    ) -> Self {
        Self {
            client_port,
            mode: ClientConnectionMode::Direct {
                client_id,
                server_addr,
                cert_digest,
            },
        }
    }

    #[cfg(feature = "bevygap")]
    pub(crate) fn bevygap(client_port: u16) -> Self {
        Self {
            client_port,
            mode: ClientConnectionMode::Bevygap,
        }
    }
}

pub(crate) struct ClientConnectionPlugin {
    pub(crate) config: ClientConnectionConfig,
}

impl Plugin for ClientConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>();
        let tick_duration = app
            .world()
            .resource::<GameConfig>()
            .movement
            .tick_duration();
        app.add_plugins(ClientPlugins { tick_duration });
        app.insert_resource(self.config.clone());
        app.add_systems(Startup, spawn_client);
    }
}

fn spawn_client(
    mut commands: Commands,
    config: Res<ClientConnectionConfig>,
    game_config: Res<GameConfig>,
) -> Result {
    let input_delay = &game_config.network.input_delay;
    let client_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), config.client_port);

    let mut client = commands.spawn((
        Client::default(),
        Link::new(None),
        LocalAddr(client_addr),
        ReplicationReceiver::default(),
        PredictionManager::default(),
        InputTimelineConfig::default().with_input_delay(LightyearInputDelayConfig {
            minimum_input_delay_ticks: input_delay.minimum_input_delay_ticks,
            maximum_input_delay_before_prediction: input_delay
                .maximum_input_delay_before_prediction_ticks,
            maximum_predicted_ticks: input_delay.maximum_predicted_ticks,
        }),
        Name::from("Client"),
    ));

    let client_entity = client.id();
    match &config.mode {
        ClientConnectionMode::Direct {
            client_id,
            server_addr,
            cert_digest,
        } => {
            let netcode_identity = NetcodeIdentity::from_env_or_dev_defaults();
            let auth = Authentication::Manual {
                server_addr: *server_addr,
                client_id: *client_id,
                private_key: netcode_identity.private_key,
                protocol_id: netcode_identity.protocol_id,
            };
            let netcode_config = NetcodeConfig {
                client_timeout_secs: 3,
                token_expire_secs: -1,
                ..default()
            };
            client.insert((
                PeerAddr(*server_addr),
                NetcodeClient::new(auth, netcode_config)?,
                WebTransportClientIo {
                    certificate_digest: normalize_certificate_digest(cert_digest),
                },
            ));
            commands.trigger(Connect {
                entity: client_entity,
            });
        }
        #[cfg(feature = "bevygap")]
        ClientConnectionMode::Bevygap => {
            info!("Spawned unconnected Lightyear client; waiting for Bevygap matchmaker token");
        }
    }
    Ok(())
}

fn normalize_certificate_digest(digest: &str) -> String {
    digest
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != ':')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalize_certificate_digest;

    #[test]
    fn normalizes_logged_certificate_digest() {
        assert_eq!(normalize_certificate_digest("5f:00:20:1e\n"), "5f00201e");
    }
}
