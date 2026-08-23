use std::net::{Ipv4Addr, SocketAddr};

use bevy::prelude::*;
use lightyear::connection::server::Start;
use lightyear::core::time::Instant;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::{ClientOf, NetcodeConfig, ServerPlugins, WebTransportServerIo};
use lightyear::prelude::*;
use lightyear::webtransport::prelude::Identity;

use shared::config::GameConfig;
use shared::network::config::{recv_link_conditioner, transport_compression, NetcodeIdentity};

#[derive(Resource, Clone, Copy)]
pub(crate) struct ServerConnectionConfig {
    pub(crate) port: u16,
    pub(crate) start_immediately: bool,
}

pub(crate) struct ServerConnectionPlugin {
    pub(crate) config: ServerConnectionConfig,
}

impl Plugin for ServerConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>();
        let tick_duration = app
            .world()
            .resource::<GameConfig>()
            .movement
            .tick_duration();
        let replication_send_interval = app
            .world()
            .resource::<GameConfig>()
            .network
            .replication_send_interval();
        app.add_plugins(ServerPlugins { tick_duration });
        app.insert_resource(ReplicationMetadata::new(replication_send_interval));
        app.register_required_components::<ClientOf, ReplicationSender>();
        app.insert_resource(self.config);
        app.add_systems(Startup, start_server);
        app.add_observer(apply_server_link_conditioner);
        app.add_observer(apply_transport_compression);
    }
}

fn start_server(mut commands: Commands, config: Res<ServerConnectionConfig>) {
    let server_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), config.port);
    let netcode_identity = NetcodeIdentity::from_env_or_dev_defaults();
    let certificate = webtransport_identity();
    let certificate_digest = certificate.certificate_chain().as_slice()[0]
        .hash()
        .to_string();
    info!("Starting WebTransport server on {server_addr}");
    info!("WebTransport certificate digest: {certificate_digest}");
    let server = commands.spawn((
        NetcodeServer::new(NetcodeConfig {
            protocol_id: netcode_identity.protocol_id,
            private_key: netcode_identity.private_key,
            ..default()
        }),
        LocalAddr(server_addr),
        WebTransportServerIo { certificate },
        Name::from("Server"),
    ));
    let server = server.id();
    if config.start_immediately {
        commands.trigger(Start { entity: server });
    } else {
        info!("Deferring WebTransport server start until matchmaker admission is ready");
    }
}

fn apply_server_link_conditioner(
    trigger: On<Add, LinkOf>,
    config: Res<GameConfig>,
    mut links: Query<&mut Link>,
) {
    let Some(conditioner) = recv_link_conditioner(&config.network) else {
        return;
    };
    let Ok(mut link) = links.get_mut(trigger.entity) else {
        return;
    };
    if link.recv.conditioner.is_some() {
        return;
    }

    let queued_packets = link.recv.drain().collect::<Vec<_>>();
    link.recv.conditioner = Some(conditioner);
    for packet in queued_packets {
        link.recv.push(packet, Instant::now());
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

fn webtransport_identity() -> Identity {
    let mut sans = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Ok(public_ip) = std::env::var("ARBITRIUM_PUBLIC_IP") {
        sans.push(public_ip);
        sans.push("*.pr.edgegap.net".to_string());
    }
    if let Ok(extra_sans) = std::env::var("SELF_SIGNED_SANS") {
        sans.extend(
            extra_sans
                .split(',')
                .map(str::trim)
                .filter(|san| !san.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    Identity::self_signed(sans).expect("failed to generate WebTransport self-signed certificate")
}
