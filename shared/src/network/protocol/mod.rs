use bevy::prelude::default;
use lightyear::prelude::*;

pub mod components;
pub mod messages;
pub mod inputs;
pub mod channels;

pub use components::{Components, ComponentsKind};
pub use messages::Messages;
pub use inputs::PlayerMovement;

pub mod prelude {
    pub use super::components::snake::*;
    pub use super::components::player::*;

    pub use super::inputs::PlayerMovement;
}

protocolize!(
    Self = GameProtocol,
    Message = Messages,
    Component = Components,
    LeafwingInput1 = PlayerMovement,
);

pub fn protocol() -> GameProtocol {
    let mut protocol = GameProtocol::default();
    protocol.add_channel::<channels::Channel1>(ChannelSettings {
        mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
        ..default()
    });
    protocol
}
