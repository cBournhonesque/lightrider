use bevy::prelude::*;
use lightyear::input::config::InputConfig;
use lightyear::prelude::input::bei::InputPlugin;
use lightyear::prelude::input::InputRegistryExt;
use lightyear::prelude::*;

pub use inputs::{
    spawn_player_input_actions, spawn_snake_input_actions, MoveSnake, PlayerInput, ServerAction,
    SnakeInput, SpawnPlayer,
};

pub mod channels;
pub mod components;
pub mod inputs;
pub mod messages;

pub mod prelude {
    // components
    pub use super::components::common::*;
    pub use super::components::food::*;
    pub use super::components::player::*;
    pub use super::components::snake::*;
    // messages
    pub use super::messages::food::*;
    pub use super::messages::room::*;
    pub use super::messages::snake::*;
    // inputs
    pub use super::inputs::*;
    // channels
    pub use super::channels::GameChannel;
}

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputPlugin::<SnakeInput> {
            config: InputConfig {
                rebroadcast_inputs: false,
                ..default()
            },
        });
        app.add_plugins(InputPlugin::<PlayerInput> {
            config: InputConfig {
                rebroadcast_inputs: false,
                ..default()
            },
        });
        app.register_input_action::<MoveSnake>();
        app.register_input_action::<SpawnPlayer>();

        app.register_message::<messages::snake::PlayerDeath>()
            .add_map_entities()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<messages::food::FoodCollision>()
            .add_map_entities()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<messages::room::RoomJoinRequest>()
            .add_direction(NetworkDirection::ClientToServer);

        app.add_channel::<channels::GameChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.register_component::<components::snake::TailPoints>()
            .add_prediction()
            .register_interpolation_fn(components::snake::interpolate_tail_points)
            .add_custom_interpolation();
        app.register_component::<components::snake::TailLength>()
            .add_prediction()
            .register_interpolation_fn(components::snake::interpolate_tail_length)
            .add_custom_interpolation();
        app.register_component::<components::snake::Speed>()
            .add_prediction();
        app.register_component::<components::snake::Acceleration>()
            .add_prediction();
        app.register_component::<components::snake::HasPlayer>()
            .add_map_entities()
            .add_prediction();
        app.register_component::<inputs::SnakeInput>()
            .add_prediction();

        app.register_component::<components::player::Player>()
            .add_map_entities();
        app.register_component::<components::player::PlayerScore>();
        app.register_component::<components::player::PlayerRank>();
        app.register_component::<components::player::PlayerStatus>();
        app.register_component::<components::food::FoodMarker>();
        app.register_component::<components::common::Position>();
        app.register_component::<components::common::RoomId>();
    }
}
