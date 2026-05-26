use std::net::SocketAddr;

use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::Client;

use crate::network::inputs::NetworkInputsPlugin;
use crate::network::interpolation::InterpolationPlugin;

pub(crate) mod config;
mod connect;
pub(crate) mod inputs;
mod interpolation;

pub(crate) struct NetworkPlugin {
    pub(crate) client_id: u64,
    pub(crate) client_port: u16,
    pub(crate) server_addr: SocketAddr,
    pub(crate) certificate_digest: String,
}

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(config::ClientConnectionPlugin {
            config: config::ClientConnectionConfig {
                client_id: self.client_id,
                client_port: self.client_port,
                server_addr: self.server_addr,
                certificate_digest: self.certificate_digest.clone(),
            },
        });
        app.add_plugins(NetworkInputsPlugin);
        app.add_plugins(InterpolationPlugin);
        app.add_observer(log_connected);
    }
}

fn log_connected(trigger: On<Add, Connected>, clients: Query<(), With<Client>>) {
    if clients.get(trigger.entity).is_ok() {
        info!("Client connected to server");
    }
}
