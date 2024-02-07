use bevy::app::{App, Plugin};
use bevy::prelude::*;
use leafwing_input_manager::prelude::{ActionState, InputMap};
use lightyear::client::input_leafwing::LeafwingInputPlugin;
use lightyear::prelude::client::*;
use shared::network::protocol::{GameProtocol, PlayerMovement};
use shared::network::protocol::prelude::*;

pub struct NetworkInputsPlugin;

impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(LeafwingInputPlugin::<GameProtocol, PlayerMovement>::new(LeafwingInputConfig {
            send_diffs_only: true,
            ..default()
        }));

        // systems
        app.add_systems(Update, add_player_inputs);
    }
}

fn add_player_inputs(
    mut commands: Commands,
    predicted_snakes: Query<Entity, (Added<HeadPoint>, With<Predicted>)>
) {
    for entity in predicted_snakes.iter() {
        commands.entity(entity).insert(
            (InputMap::new([
                (KeyCode::Right, PlayerMovement::Right),
                (KeyCode::D, PlayerMovement::Right),
                (KeyCode::Left, PlayerMovement::Left),
                (KeyCode::A, PlayerMovement::Left),
                (KeyCode::Up, PlayerMovement::Up),
                (KeyCode::W, PlayerMovement::Up),
                (KeyCode::Down, PlayerMovement::Down),
                (KeyCode::S, PlayerMovement::Down),
            ]), ActionState::<PlayerMovement>::default())
        );
    }
}