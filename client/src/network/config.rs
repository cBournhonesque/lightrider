use bevy::prelude::*;
use lightyear::core::tick::TickDuration;
use lightyear::interpolation::timeline::InterpolationConfig;
use lightyear::link::LinkStats;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeClient;
use lightyear::prelude::client::{
    ClientPlugins, InputDelayConfig as LightyearInputDelayConfig, InputTimelineConfig,
    WebTransportClientIo,
};
use lightyear::prelude::*;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use shared::config::GameConfig;
use shared::config::InputDelayConfig;
use shared::network::config::{recv_link_conditioner, transport_compression, NetcodeIdentity};

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
    #[cfg(feature = "lightyear-matchmaker")]
    Matchmaker,
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

    #[cfg(feature = "lightyear-matchmaker")]
    pub(crate) fn matchmaker(client_port: u16) -> Self {
        Self {
            client_port,
            mode: ClientConnectionMode::Matchmaker,
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
        app.add_systems(Update, disconnect_when_prediction_budget_exceeded);
        app.add_observer(apply_transport_compression);
    }
}

fn spawn_client(
    mut commands: Commands,
    config: Res<ClientConnectionConfig>,
    game_config: Res<GameConfig>,
) -> Result {
    let input_delay = &game_config.network.input_delay;
    let interpolation_delay = &game_config.network.interpolation_delay;
    let client_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), config.client_port);

    let mut client = commands.spawn((
        Client::default(),
        Link::new(recv_link_conditioner(&game_config.network)),
        LocalAddr(client_addr),
        ReplicationReceiver::default(),
        PredictionManager::default(),
        InputTimelineConfig::default().with_input_delay(LightyearInputDelayConfig {
            minimum_input_delay_ticks: input_delay.minimum_input_delay_ticks,
            maximum_input_delay_before_prediction: input_delay
                .maximum_input_delay_before_prediction_ticks,
            maximum_predicted_ticks: input_delay.maximum_predicted_ticks,
        }),
        InterpolationConfig::default()
            .with_min_delay(interpolation_delay.min_delay())
            .with_send_interval_ratio(interpolation_delay.send_interval_ratio),
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
        #[cfg(feature = "lightyear-matchmaker")]
        ClientConnectionMode::Matchmaker => {
            info!("Spawned unconnected Lightyear client; waiting for matchmaker token");
        }
    }
    Ok(())
}

fn disconnect_when_prediction_budget_exceeded(
    mut commands: Commands,
    game_config: Res<GameConfig>,
    tick_duration: Res<TickDuration>,
    clients: Query<(Entity, &Link), (With<Client>, With<Connected>)>,
) {
    let input_delay = &game_config.network.input_delay;
    for (entity, link) in &clients {
        let required_prediction_ticks =
            required_prediction_ticks(link.stats, tick_duration.0, input_delay);
        if required_prediction_ticks <= input_delay.maximum_predicted_ticks {
            continue;
        }

        warn!(
            entity = ?entity,
            rtt_ms = link.stats.rtt.as_secs_f64() * 1000.0,
            jitter_ms = link.stats.jitter.as_secs_f64() * 1000.0,
            required_prediction_ticks,
            maximum_predicted_ticks = input_delay.maximum_predicted_ticks,
            maximum_input_delay_before_prediction_ticks = input_delay
                .maximum_input_delay_before_prediction_ticks,
            "disconnecting client because latency exceeds prediction budget"
        );
        commands.trigger(Disconnect { entity });
    }
}

fn required_prediction_ticks(
    link_stats: LinkStats,
    tick_duration: Duration,
    input_delay: &InputDelayConfig,
) -> u16 {
    effective_rtt_ticks(link_stats, tick_duration)
        .saturating_sub(input_delay.maximum_input_delay_before_prediction_ticks)
}

fn effective_rtt_ticks(link_stats: LinkStats, tick_duration: Duration) -> u16 {
    let sync_config = SyncConfig::default();
    let effective_rtt = link_stats
        .rtt
        .saturating_add(sync_config.jitter_margin(link_stats.jitter, tick_duration));
    ceil_duration_ticks(effective_rtt, tick_duration)
}

fn ceil_duration_ticks(duration: Duration, tick_duration: Duration) -> u16 {
    let tick_nanos = tick_duration.as_nanos();
    if tick_nanos == 0 {
        return u16::MAX;
    }
    let ticks = duration
        .as_nanos()
        .saturating_add(tick_nanos.saturating_sub(1))
        / tick_nanos;
    ticks.try_into().unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THIRTY_TWO_HZ_TICK: Duration = Duration::from_nanos(1_000_000_000 / 32);

    #[test]
    fn default_input_delay_covers_roughly_sixty_ms_then_predicts() {
        let input_delay = InputDelayConfig::balanced();

        assert_eq!(input_delay.minimum_input_delay_ticks, 0);
        assert_eq!(input_delay.maximum_input_delay_before_prediction_ticks, 2);
        assert_eq!(input_delay.maximum_predicted_ticks, 8);
    }

    #[test]
    fn prediction_budget_allows_eight_ticks_beyond_input_delay() {
        let input_delay = InputDelayConfig::balanced();
        let stats = LinkStats {
            // Effective RTT includes Lightyear's default one-tick jitter margin.
            rtt: THIRTY_TWO_HZ_TICK * 9,
            jitter: Duration::ZERO,
        };

        assert_eq!(
            required_prediction_ticks(stats, THIRTY_TWO_HZ_TICK, &input_delay),
            8
        );
    }

    #[test]
    fn prediction_budget_exceeds_after_eight_ticks_beyond_input_delay() {
        let input_delay = InputDelayConfig::balanced();
        let stats = LinkStats {
            rtt: THIRTY_TWO_HZ_TICK * 10,
            jitter: Duration::ZERO,
        };

        assert_eq!(
            required_prediction_ticks(stats, THIRTY_TWO_HZ_TICK, &input_delay),
            9
        );
    }

    #[test]
    fn normalizes_logged_certificate_digest() {
        assert_eq!(normalize_certificate_digest("5f:00:20:1e\n"), "5f00201e");
    }
}

fn apply_transport_compression(
    trigger: On<Add, Transport>,
    config: Res<GameConfig>,
    mut transports: Query<&mut Transport>,
) {
    let Ok(mut transport) = transports.get_mut(trigger.entity) else {
        return;
    };
    transport.set_compression(transport_compression(&config.network));
}

pub(crate) fn normalize_certificate_digest(digest: &str) -> String {
    digest
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != ':')
        .collect()
}
