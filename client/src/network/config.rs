use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use bevy::prelude::default;
use lightyear::prelude::*;
use lightyear::prelude::client::*;
use shared::network::config::{KEY, PROTOCOL_ID, shared_config, Transports};
use shared::network::protocol::{GameProtocol, protocol};

pub(crate) fn build_plugin(
    client_id: ClientId,
    client_port: u16,
    server_addr: SocketAddr,
    transport: Transports,
) -> ClientPlugin<GameProtocol> {
    let auth = Authentication::Manual {
        server_addr,
        client_id,
        private_key: KEY,
        protocol_id: PROTOCOL_ID,
    };
    let client_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), client_port);
    let certificate_digest =
        String::from("6c594425dd0c8664c188a0ad6e641b39ff5f007e5bcfc1e72c7a7f2f38ecf819")
            .replace(":", "");
    let transport_config = match transport {
        #[cfg(not(target_family = "wasm"))]
        Transports::Udp => TransportConfig::UdpSocket(client_addr),
        Transports::WebTransport => TransportConfig::WebTransportClient {
            client_addr,
            server_addr,
            #[cfg(target_family = "wasm")]
            certificate_digest,
        },
        #[cfg(not(target_family = "wasm"))]
        Transports::WebSocket => TransportConfig::WebSocketClient { server_addr },
    };
    let link_conditioner = LinkConditionerConfig {
        incoming_latency: Duration::from_millis(100),
        incoming_jitter: Duration::from_millis(40),
        incoming_loss: 0.05,
    };
    let io = Io::from_config(
        IoConfig::from_transport(transport_config).with_conditioner(link_conditioner),
    );
    let config = ClientConfig {
        shared: shared_config(),
        net: NetConfig::Netcode {
            auth,
            config: NetcodeConfig::default(),
        },
        interpolation: InterpolationConfig {
            delay: InterpolationDelay::default().with_send_interval_ratio(2.0),
            // do not do linear interpolation per component, instead we provide our own interpolation logic
            custom_interpolation_logic: true,
        },
        ..default()
    };
    ClientPlugin::new(PluginConfig::new(config, io, protocol()))
}