use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

use crate::network::inputs::NetworkInputsPlugin;

mod config;
mod connection_events;
mod inputs;

pub(crate) struct NetworkPluginGroup {
    pub(crate) port: u16,
}

impl PluginGroup for NetworkPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(config::ServerConnectionPlugin {
                config: config::ServerConnectionConfig { port: self.port },
            })
            .add(NetworkPlugin)
    }
}

impl NetworkPluginGroup {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(NetworkInputsPlugin);

        // systems
        app.add_observer(connection_events::handle_new_client);
        app.add_observer(connection_events::handle_new_client_of);
        app.add_observer(connection_events::handle_connected);
    }
}
