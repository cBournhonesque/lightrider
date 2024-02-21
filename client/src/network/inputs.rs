use bevy::app::{App, Plugin};
use bevy::prelude::*;
use leafwing_input_manager::prelude::{ActionState, InputMap};
use lightyear::client::input_leafwing::LeafwingInputPlugin;
use lightyear::prelude::client::*;

use shared::network::protocol::{GameProtocol, PlayerMovement};
use shared::network::protocol::prelude::*;
use crate::inputs::LocalInput;

pub struct NetworkInputsPlugin;

impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(LeafwingInputPlugin::<GameProtocol, PlayerMovement>::new(LeafwingInputConfig {
            send_diffs_only: true,
            ..default()
        }));
        // TODO: I only want to run this system if the player is dead!
        //  need to allow the user to configure the state in which the system runs
        //  maybe provide an optional SystemSet as input, in which case all the plugin's systems will be added to that set?
        app.add_plugins(LeafwingInputPlugin::<GameProtocol, DeadGameAction>::new(LeafwingInputConfig {
            send_diffs_only: true,
            ..default()
        }));

        // systems
        app.add_systems(Update, (add_game_inputs, add_movement_inputs));
    }
}


// TODO: move somewhere else?
/// Component that indicates that the entity is owned by the local client
#[derive(Component)]
pub struct Owned;

fn add_game_inputs(
    mut commands: Commands,
    client: Res<ClientConnection>,
    players: Query<(Entity, Ref<Player>)>,
) {
    for (entity, player) in players.iter() {
        if player.is_added() && player.id == client.id() {
            commands.entity(entity).insert(
                (
                    InputMap::new([
                        (DeadGameAction::Spawn, KeyCode::Enter),
                    ]),
                    InputMap::new([
                        (LocalInput::ToggleCamera, KeyCode::KeyT),
                    ]),
                    ActionState::<DeadGameAction>::default(),
                    ActionState::<LocalInput>::default(),
                    Owned
                )
            );
        }
    }
}


fn add_movement_inputs(
    mut commands: Commands,
    predicted_snakes: Query<Entity, (Added<TailPoints>, With<Predicted>)>
) {
    for entity in predicted_snakes.iter() {
        commands.entity(entity).insert(
            (InputMap::new([
                (PlayerMovement::Right, KeyCode::ArrowRight),
                (PlayerMovement::Right, KeyCode::KeyD),
                (PlayerMovement::Left, KeyCode::ArrowLeft),
                (PlayerMovement::Left, KeyCode::KeyA),
                (PlayerMovement::Up, KeyCode::ArrowUp),
                (PlayerMovement::Up, KeyCode::KeyW),
                (PlayerMovement::Down, KeyCode::ArrowDown),
                (PlayerMovement::Down, KeyCode::KeyS),
            ]), ActionState::<PlayerMovement>::default())
        );
    }
}