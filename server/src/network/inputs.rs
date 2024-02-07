use bevy::app::{App, Plugin};
use bevy::prelude::default;
use lightyear::server::input_leafwing::{LeafwingInputPlugin};
use shared::network::protocol::{GameProtocol, PlayerMovement};

pub struct NetworkInputsPlugin;


impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(LeafwingInputPlugin::<GameProtocol, PlayerMovement>::default());
    }
}