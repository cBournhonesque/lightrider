use std::net::{Ipv4Addr, SocketAddr};

use bevy::prelude::*;
use lightyear::connection::server::Start;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::{ClientOf, NetcodeConfig, ServerPlugins, WebTransportServerIo};
use lightyear::prelude::*;

use shared::config::GameConfig;
use shared::network::config::{KEY, PROTOCOL_ID};

#[derive(Resource, Clone, Copy)]
pub(crate) struct ServerConnectionConfig {
    pub(crate) port: u16,
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
        app.add_plugins(ServerPlugins { tick_duration });
        app.register_required_components::<ClientOf, ReplicationSender>();
        app.insert_resource(self.config);
        app.add_systems(Startup, start_server);
    }
}

fn start_server(mut commands: Commands, config: Res<ServerConnectionConfig>) {
    let server_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), config.port);
    info!("Starting WebTransport server on {server_addr}");
    let mut server = commands.spawn((
        NetcodeServer::new(NetcodeConfig {
            protocol_id: PROTOCOL_ID,
            private_key: KEY,
            ..default()
        }),
        LocalAddr(server_addr),
        Name::from("Server"),
    ));

    let identity = Identity::self_signed(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .expect("self-signed WebTransport certificate should be valid");
    let digest = identity.certificate_chain().as_slice()[0].hash();
    info!("Generated self-signed WebTransport certificate digest: {digest}");
    server.insert(WebTransportServerIo {
        certificate: identity,
    });

    let server = server.id();
    commands.trigger(Start { entity: server });
}
