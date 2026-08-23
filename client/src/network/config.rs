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

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct ClientConnectionStatus {
    pub(crate) disconnect_reason: Option<String>,
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
        app.init_resource::<ClientConnectionStatus>();
        app.add_systems(Startup, spawn_client);
        app.add_systems(Update, disconnect_when_prediction_budget_exceeded);
        app.add_observer(clear_disconnect_reason_on_connect);
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
                    target: None,
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
    time: Res<Time>,
    tick_duration: Res<TickDuration>,
    mut connection_status: ResMut<ClientConnectionStatus>,
    clients: Query<(Entity, &Link), (With<Client>, With<Connected>)>,
    mut next_trace_at: Local<f64>,
    mut next_budget_warning_at: Local<f64>,
) {
    let input_delay = &game_config.network.input_delay;
    let now = time.elapsed_secs_f64();
    let should_trace = now >= *next_trace_at;
    if should_trace {
        *next_trace_at = now + 0.25;
    }

    for (entity, link) in &clients {
        let budget = prediction_budget(link.stats, tick_duration.0, input_delay);
        if should_trace {
            let sync_config = SyncConfig::default();
            let jitter_margin = sync_config.jitter_margin(link.stats.jitter, tick_duration.0);
            let effective_rtt = link.stats.rtt.saturating_add(jitter_margin);
            tracing::trace!(
                target: "lightyear_debug::manual",
                kind = "prediction_budget",
                sample_point = "Update",
                schedule = "Update",
                entity = ?entity,
                rtt_ms = link.stats.rtt.as_secs_f64() * 1000.0,
                jitter_ms = link.stats.jitter.as_secs_f64() * 1000.0,
                jitter_margin_ms = jitter_margin.as_secs_f64() * 1000.0,
                effective_rtt_ms = effective_rtt.as_secs_f64() * 1000.0,
                effective_rtt_ticks = budget.effective_rtt_ticks,
                input_delay_ticks = budget.input_delay_ticks,
                predicted_ticks = budget.predicted_ticks,
                maximum_predicted_ticks = input_delay.maximum_predicted_ticks,
                maximum_input_delay_before_prediction_ticks = input_delay
                    .maximum_input_delay_before_prediction_ticks,
                maximum_input_delay_ticks = input_delay.maximum_input_delay_ticks,
                "prediction budget sample"
            );
        }

        if budget.input_delay_ticks <= input_delay.maximum_input_delay_ticks {
            continue;
        }

        let reason = format!(
            "Disconnected because network latency exceeded the prediction budget. \
             RTT {:.1}ms, jitter {:.1}ms, effective latency {} ticks, input delay needed {} ticks \
             but max allowed input delay is {} ticks, predicted ticks {} / {}.",
            link.stats.rtt.as_secs_f64() * 1000.0,
            link.stats.jitter.as_secs_f64() * 1000.0,
            budget.effective_rtt_ticks,
            budget.input_delay_ticks,
            input_delay.maximum_input_delay_ticks,
            budget.predicted_ticks,
            input_delay.maximum_predicted_ticks,
        );
        if !input_delay.disconnect_on_prediction_budget_exceeded {
            if now >= *next_budget_warning_at {
                *next_budget_warning_at = now + 2.0;
                warn!(
                    entity = ?entity,
                    rtt_ms = link.stats.rtt.as_secs_f64() * 1000.0,
                    jitter_ms = link.stats.jitter.as_secs_f64() * 1000.0,
                    effective_rtt_ticks = budget.effective_rtt_ticks,
                    input_delay_ticks = budget.input_delay_ticks,
                    predicted_ticks = budget.predicted_ticks,
                    maximum_predicted_ticks = input_delay.maximum_predicted_ticks,
                    maximum_input_delay_before_prediction_ticks = input_delay
                        .maximum_input_delay_before_prediction_ticks,
                    maximum_input_delay_ticks = input_delay.maximum_input_delay_ticks,
                    reason = %reason,
                    "prediction budget exceeded; continuing because disconnect is disabled"
                );
            }
            continue;
        }

        warn!(
            entity = ?entity,
            rtt_ms = link.stats.rtt.as_secs_f64() * 1000.0,
            jitter_ms = link.stats.jitter.as_secs_f64() * 1000.0,
            effective_rtt_ticks = budget.effective_rtt_ticks,
            input_delay_ticks = budget.input_delay_ticks,
            predicted_ticks = budget.predicted_ticks,
            maximum_predicted_ticks = input_delay.maximum_predicted_ticks,
            maximum_input_delay_before_prediction_ticks = input_delay
                .maximum_input_delay_before_prediction_ticks,
            maximum_input_delay_ticks = input_delay.maximum_input_delay_ticks,
            reason = %reason,
            "disconnecting client because latency exceeds prediction budget"
        );
        connection_status.disconnect_reason = Some(reason);
        commands.trigger(Disconnect { entity });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PredictionBudget {
    effective_rtt_ticks: u16,
    input_delay_ticks: u16,
    predicted_ticks: u16,
}

fn prediction_budget(
    link_stats: LinkStats,
    tick_duration: Duration,
    input_delay: &InputDelayConfig,
) -> PredictionBudget {
    let effective_rtt_ticks = effective_rtt_ticks(link_stats, tick_duration);
    let input_delay_ticks = adaptive_input_delay_ticks(effective_rtt_ticks, input_delay);
    PredictionBudget {
        effective_rtt_ticks,
        input_delay_ticks,
        predicted_ticks: effective_rtt_ticks.saturating_sub(input_delay_ticks),
    }
}

fn adaptive_input_delay_ticks(effective_rtt_ticks: u16, input_delay: &InputDelayConfig) -> u16 {
    if effective_rtt_ticks <= input_delay.minimum_input_delay_ticks {
        input_delay.minimum_input_delay_ticks
    } else if effective_rtt_ticks <= input_delay.maximum_input_delay_before_prediction_ticks {
        effective_rtt_ticks
    } else if effective_rtt_ticks
        <= input_delay
            .maximum_input_delay_before_prediction_ticks
            .saturating_add(input_delay.maximum_predicted_ticks)
    {
        input_delay.maximum_input_delay_before_prediction_ticks
    } else {
        effective_rtt_ticks.saturating_sub(input_delay.maximum_predicted_ticks)
    }
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

fn clear_disconnect_reason_on_connect(
    trigger: On<Add, Connected>,
    clients: Query<(), With<Client>>,
    mut connection_status: ResMut<ClientConnectionStatus>,
) {
    if clients.get(trigger.entity).is_ok() {
        connection_status.disconnect_reason = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THIRTY_TWO_HZ_TICK: Duration = Duration::from_nanos(1_000_000_000 / 32);

    #[test]
    fn default_input_delay_delays_then_predicts_then_delays_more() {
        let input_delay = InputDelayConfig::balanced();

        assert_eq!(input_delay.minimum_input_delay_ticks, 0);
        assert_eq!(input_delay.maximum_input_delay_before_prediction_ticks, 3);
        assert_eq!(input_delay.maximum_predicted_ticks, 10);
        assert_eq!(input_delay.maximum_input_delay_ticks, 6);
    }

    #[test]
    fn prediction_budget_allows_ten_ticks_beyond_initial_input_delay() {
        let input_delay = InputDelayConfig::balanced();
        let stats = LinkStats {
            // Effective RTT includes Lightyear's default one-tick jitter margin.
            rtt: THIRTY_TWO_HZ_TICK * 12,
            jitter: Duration::ZERO,
        };

        assert_eq!(
            prediction_budget(stats, THIRTY_TWO_HZ_TICK, &input_delay),
            PredictionBudget {
                effective_rtt_ticks: 13,
                input_delay_ticks: 3,
                predicted_ticks: 10,
            }
        );
    }

    #[test]
    fn prediction_budget_adds_more_input_delay_after_ten_predicted_ticks() {
        let input_delay = InputDelayConfig::balanced();
        let stats = LinkStats {
            rtt: THIRTY_TWO_HZ_TICK * 15,
            jitter: Duration::ZERO,
        };

        assert_eq!(
            prediction_budget(stats, THIRTY_TWO_HZ_TICK, &input_delay),
            PredictionBudget {
                effective_rtt_ticks: 16,
                input_delay_ticks: 6,
                predicted_ticks: 10,
            }
        );
    }

    #[test]
    fn prediction_budget_exceeds_after_six_total_input_delay_ticks() {
        let input_delay = InputDelayConfig::balanced();
        let stats = LinkStats {
            rtt: THIRTY_TWO_HZ_TICK * 16,
            jitter: Duration::ZERO,
        };

        let budget = prediction_budget(stats, THIRTY_TWO_HZ_TICK, &input_delay);

        assert_eq!(
            budget,
            PredictionBudget {
                effective_rtt_ticks: 17,
                input_delay_ticks: 7,
                predicted_ticks: 10,
            }
        );
        assert!(budget.input_delay_ticks > input_delay.maximum_input_delay_ticks);
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
