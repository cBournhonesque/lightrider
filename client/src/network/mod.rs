use std::net::SocketAddr;

use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::ClientId;

use shared::network::config::Transports;

use crate::network::inputs::NetworkInputsPlugin;
use crate::network::interpolation::InterpolationPlugin;

pub(crate) mod config;
mod inputs;
mod interpolation;

pub(crate) struct NetworkPlugin {
    pub(crate) client_id: ClientId,
    pub(crate) client_port: u16,
    pub(crate) server_addr: SocketAddr,
    pub(crate) transport: Transports,
}

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(config::build_plugin(
            self.client_id,
            self.client_port,
            self.server_addr,
            self.transport,
        ));
        app.add_plugins(NetworkInputsPlugin);
        app.add_plugins(InterpolationPlugin);
        app.add_systems(Startup, connect);
    }
}

fn connect(mut net: ResMut<ClientConnection>) {
    if net.is_connected() {
        return;
    }
    let _ = net.connect();
}
