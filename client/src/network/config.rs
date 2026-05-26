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
use shared::network::config::{KEY, PROTOCOL_ID};

#[derive(Resource, Clone)]
pub(crate) struct ClientConnectionConfig {
    pub(crate) client_id: u64,
    pub(crate) client_port: u16,
    pub(crate) server_addr: SocketAddr,
    pub(crate) certificate_digest: String,
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
    let auth = Authentication::Manual {
        server_addr: config.server_addr,
        client_id: config.client_id,
        private_key: KEY,
        protocol_id: PROTOCOL_ID,
    };

    let mut client = commands.spawn((
        Client::default(),
        Link::new(None),
        LocalAddr(client_addr),
        PeerAddr(config.server_addr),
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

    let netcode_config = NetcodeConfig {
        client_timeout_secs: 3,
        token_expire_secs: -1,
        ..default()
    };
    client.insert(NetcodeClient::new(auth, netcode_config)?);

    client.insert(WebTransportClientIo {
        certificate_digest: config.certificate_digest.clone(),
    });

    let client = client.id();
    commands.trigger(Connect { entity: client });
    Ok(())
}
