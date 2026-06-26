use bevy::prelude::*;
use lightyear::input::config::InputConfig;
use lightyear::prelude::input::bei::InputPlugin;
use lightyear::prelude::input::InputRegistryExt;
use lightyear::prelude::*;

use crate::config::GameConfig;

pub use inputs::{spawn_snake_input_actions, MoveSnake, ServerAction, SnakeInput};

pub mod channels;
pub mod components;
pub mod inputs;
pub mod messages;

pub mod prelude {
    pub use bevy_replicon::prelude::DiffIndex;

    // components
    pub use super::components::common::*;
    pub use super::components::food::*;
    pub use super::components::player::*;
    pub use super::components::snake::*;
    // messages
    pub use super::messages::admin::*;
    pub use super::messages::food::*;
    pub use super::messages::leaderboard::*;
    pub use super::messages::room::*;
    pub use super::messages::snake::*;
    // inputs
    pub use super::inputs::*;
    // channels
    pub use super::channels::{GameChannel, LeaderboardChannel};
}

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>();
        let input_packet_redundancy_ticks = app
            .world()
            .resource::<GameConfig>()
            .network
            .input_packet_redundancy_ticks;
        app.add_plugins(InputPlugin::<SnakeInput> {
            config: InputConfig {
                lag_compensation: app
                    .world()
                    .resource::<GameConfig>()
                    .network
                    .lag_compensation
                    .enabled,
                rebroadcast_inputs: false,
                packet_redundancy: input_packet_redundancy_ticks,
                ..default()
            },
        });
        app.register_input_action::<MoveSnake>();

        app.register_message::<messages::snake::PlayerDeath>()
            .add_map_entities()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<messages::food::FoodCollision>()
            .add_map_entities()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<messages::room::RoomJoinRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<messages::room::PlayerNameUpdate>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<messages::room::PlayerSpawnRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<messages::room::ClientViewportUpdate>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<messages::admin::AdminLoginRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<messages::admin::AdminCommand>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<messages::admin::AdminResponse>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<messages::leaderboard::LeaderboardSnapshot>()
            .add_map_entities()
            .add_direction(NetworkDirection::ServerToClient);

        app.add_channel::<channels::GameChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);
        app.add_channel::<channels::LeaderboardChannel>(ChannelSettings {
            mode: ChannelMode::SequencedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);

        register_components(app);
        app.add_systems(Update, inputs::cleanup_orphaned_snake_input_actions);
    }
}

fn register_components(app: &mut App) {
    // Tail visual-correction functions exist but are intentionally not registered yet.
    // Keep predicted visual correction disabled until the smoothing behavior is validated.
    app.component::<components::snake::SnakeHead>()
        .replicate()
        .predict()
        .with_rollback_condition(components::snake::snake_head_should_rollback)
        .register_interpolation_fn(components::snake::interpolate_snake_head)
        .add_custom_interpolation();
    app.component::<components::snake::TailPoints>()
        .replicate_diff()
        .predict_diff()
        .with_rollback_condition(components::snake::tail_points_should_rollback)
        .add_custom_interpolation_diff();
    app.component::<components::snake::TailLength>()
        .replicate()
        .predict()
        .with_rollback_condition(components::snake::tail_length_should_rollback)
        .register_interpolation_fn(components::snake::interpolate_tail_length)
        .add_custom_interpolation();
    app.component::<components::snake::Speed>()
        .replicate()
        .predict()
        .with_rollback_condition(components::snake::speed_should_rollback);
    app.component::<components::snake::Acceleration>()
        .replicate()
        .predict()
        .with_rollback_condition(components::snake::acceleration_should_rollback);
    app.component::<components::snake::FoodBoost>()
        .replicate()
        .predict()
        .with_rollback_condition(components::snake::food_boost_should_rollback);
    app.component::<components::snake::HasPlayer>()
        .replicate()
        .predict()
        .add_custom_interpolation();
    app.component::<inputs::SnakeInput>().replicate().predict();

    app.component::<components::player::Player>().replicate();
    app.component::<components::player::PlayerScore>()
        .replicate();
    app.component::<components::player::PlayerStatus>()
        .replicate();
    app.component::<components::food::FoodMarker>().replicate();
    app.component::<components::common::Position>()
        .replicate()
        .add_interpolation_with(components::common::interpolate_position);
    app.component::<components::common::RoomId>().replicate();
}
