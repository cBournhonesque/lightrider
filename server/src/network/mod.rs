pub(crate) mod bundle;
mod config;
mod events;
mod inputs;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

use lightyear::prelude::server::*;
use shared::network::config::{Transports};
use shared::network::protocol::{GameProtocol};
use crate::network::inputs::NetworkInputsPlugin;


pub(crate) struct NetworkPluginGroup {
    pub(crate) lightyear: ServerPlugin<GameProtocol>,
}

impl PluginGroup for NetworkPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(self.lightyear)
            .add(NetworkPlugin)
    }
}

impl NetworkPluginGroup {
    pub async fn new(port: u16, transport: Transports) -> Self {
        let lightyear = config::build_plugin(port, transport).await;
        Self {
            lightyear,
        }
    }
}

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(NetworkInputsPlugin);

        // systems
        app.add_systems(Update, events::handle_connections);
    }
}

