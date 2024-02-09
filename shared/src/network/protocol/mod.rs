use bevy::prelude::default;
use lightyear::prelude::*;

pub use components::{Components, ComponentsKind};
pub use inputs::{PlayerMovement, GameAction};
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
    // messages
    pub use super::messages::snake::*;
    // inputs
    pub use super::inputs::PlayerMovement;
    pub use super::inputs::GameAction;
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
    LeafwingInput2 = GameAction,
);

pub fn protocol() -> GameProtocol {
    let mut protocol = GameProtocol::default();
    protocol.add_channel::<channels::GameChannel>(ChannelSettings {
        mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
        ..default()
    });
    protocol
}
