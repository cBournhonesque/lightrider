use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

use crate::network::inputs::NetworkInputsPlugin;

mod config;
mod connection_events;
mod inputs;

pub(crate) struct NetworkPluginGroup {
    pub(crate) port: u16,
    pub(crate) start_immediately: bool,
}

impl PluginGroup for NetworkPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(config::ServerConnectionPlugin {
                config: config::ServerConnectionConfig {
                    port: self.port,
                    start_immediately: self.start_immediately,
                },
            })
            .add(NetworkPlugin)
    }
}

impl NetworkPluginGroup {
    pub fn new(port: u16, start_immediately: bool) -> Self {
        Self {
            port,
            start_immediately,
        }
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
    }
}
