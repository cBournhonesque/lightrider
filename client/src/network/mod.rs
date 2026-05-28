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
    pub(crate) connection: config::ClientConnectionConfig,
}

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(config::ClientConnectionPlugin {
            config: self.connection.clone(),
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
