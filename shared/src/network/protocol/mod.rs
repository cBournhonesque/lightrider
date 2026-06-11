use bevy::prelude::*;
use bevy_replicon::prelude::AppRuleExt as RepliconAppRuleExt;
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

        // Tail visual-correction functions exist but are intentionally not registered yet.
        // Keep predicted visual correction disabled until the smoothing behavior is validated.
        register_tail_points_diff(app);
        app.non_networked_component::<components::snake::TailPoints>()
            .add_prediction_diff()
            .add_should_rollback(components::snake::tail_points_should_rollback)
            // Install the diff interpolation writer before registering the interpolation fn,
            // otherwise the generic history writer will try to deserialize patch payloads.
            .add_custom_interpolation_diff()
            .register_interpolation_fn(components::snake::interpolate_tail_points);
        app.register_component::<components::snake::TailLength>()
            .add_prediction()
            .add_should_rollback(components::snake::tail_length_should_rollback)
            .register_interpolation_fn(components::snake::interpolate_tail_length)
            .add_custom_interpolation();
        app.register_component::<components::snake::Speed>()
            .add_prediction()
            .add_should_rollback(components::snake::speed_should_rollback);
        app.register_component::<components::snake::Acceleration>()
            .add_prediction()
            .add_should_rollback(components::snake::acceleration_should_rollback);
        app.register_component::<components::snake::FoodBoost>()
            .add_prediction()
            .add_should_rollback(components::snake::food_boost_should_rollback);
        app.register_component::<components::snake::HasPlayer>()
            .add_prediction();
        app.register_component::<inputs::SnakeInput>()
            .add_prediction();

        app.register_component::<components::player::Player>();
        app.register_component::<components::player::PlayerScore>();
        app.register_component::<components::player::PlayerStatus>();
        app.register_component::<components::food::FoodMarker>();
        app.register_component::<components::common::Position>()
            .add_interpolation_with(components::common::interpolate_position);
        app.register_component::<components::common::RoomId>();
    }
}

fn register_tail_points_diff(app: &mut App) {
    app.world_mut()
        .init_resource::<lightyear::prelude::ComponentRegistry>();
    app.world_mut().resource_scope(
        |world, mut registry: Mut<lightyear::prelude::ComponentRegistry>| {
            if !registry.is_registered::<components::snake::TailPoints>() {
                registry.register_component::<components::snake::TailPoints>(world);
            }
        },
    );
    app.replicate_diff::<components::snake::TailPoints>();
}
