use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::prelude::default;
use lightyear::prelude::{IoConfig, LinkConditionerConfig, TransportConfig};
use lightyear::prelude::server::{Certificate, NetcodeConfig, NetConfig, PluginConfig, ServerConfig, ServerPlugin};

use shared::network::config::{KEY, PROTOCOL_ID, shared_config, Transports};
use shared::network::protocol::{GameProtocol, protocol};

pub(crate) async fn build_plugin(port: u16, transport: Transports) -> ServerPlugin<GameProtocol> {
    // Step 1: create the io (transport + link conditioner)
    let server_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port);
    let transport_config = match transport {
        Transports::Udp => TransportConfig::UdpSocket(server_addr),
        // if using webtransport, we load the certificate keys
        Transports::WebTransport => {
            let certificate =
                Certificate::load("certificates/cert.pem", "certificates/key.pem")
                    .await
                    .unwrap();
            let digest = &certificate.hashes()[0];
            println!(
                    "Generated self-signed certificate with digest: {}",
                    digest
                );
            TransportConfig::WebTransportServer {
                server_addr,
                certificate,
            }
        }
        Transports::WebSocket => TransportConfig::WebSocketServer { server_addr },
    };
    let link_conditioner = LinkConditionerConfig {
        incoming_latency: Duration::from_millis(0),
        incoming_jitter: Duration::from_millis(0),
        incoming_loss: 0.0,
    };
    // Step 2: define the server configuration
    let config = ServerConfig {
        shared: shared_config(),
        net: NetConfig::Netcode {
            config: NetcodeConfig::default()
                .with_protocol_id(PROTOCOL_ID)
                .with_key(KEY),
            io: IoConfig::from_transport(transport_config).with_conditioner(link_conditioner),
        },
        ..default()
    };

    // Step 3: create the plugin
    let plugin_config = PluginConfig::new(config, protocol());
    ServerPlugin::new(plugin_config)
}