pub(crate) mod bundle;
mod config;
mod events;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::server::*;
use shared::network::config::{Transports};
use shared::network::protocol::{GameProtocol};


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
        app.add_systems(Update, events::handle_connections);
    }
}

