use bevy::prelude::default;
use lightyear::prelude::*;

pub use components::{Components, ComponentsKind};
pub use inputs::{PlayerMovement, DeadGameAction};
pub use messages::Messages;

pub mod components;
pub mod messages;
pub mod inputs;
pub mod channels;

pub mod prelude {
    pub use super::ClientConnectionManager;
    pub use super::ServerConnectionManager;
    // components
    pub use super::components::player::*;
    pub use super::components::snake::*;
    pub use super::components::food::*;
    pub use super::components::common::*;
    // messages
    pub use super::messages::snake::*;
    pub use super::messages::food::*;
    // inputs
    pub use super::inputs::PlayerMovement;
    pub use super::inputs::DeadGameAction;
    // channels
    pub use super::channels::GameChannel;
    // reexports
    pub use super::Replicate;
}

protocolize!(
    Self = GameProtocol,
    Message = Messages,
    Component = Components,
    LeafwingInput1 = PlayerMovement,
    LeafwingInput2 = DeadGameAction,
);

pub fn protocol() -> GameProtocol {
    let mut protocol = GameProtocol::default();
    protocol.add_channel::<channels::GameChannel>(ChannelSettings {
        mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
        ..default()
    });
    protocol
}
