use bevy::app::{App, Plugin};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use lightyear::prelude::input::bei::{Action, ActionOf, InputMarker};
use lightyear::prelude::{Client, Controlled, LocalId};

use shared::network::protocol::prelude::*;

pub struct NetworkInputsPlugin;

impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_player_inputs, add_snake_inputs));
    }
}

fn add_player_inputs(
    mut commands: Commands,
    client: Query<&LocalId, With<Client>>,
    players: Query<
        Entity,
        (
            With<Controlled>,
            With<Player>,
            With<PlayerInput>,
            Without<InputMarker<PlayerInput>>,
        ),
    >,
    actions: Query<&ActionOf<PlayerInput>, With<Action<SpawnPlayer>>>,
) {
    let Ok(client_id) = client.single().map(|id| id.0) else {
        return;
    };
    for player in &players {
        commands
            .entity(player)
            .insert(InputMarker::<PlayerInput>::default());
        if !actions.iter().any(|action| action.get() == player) {
            spawn_player_input_actions(&mut commands, player, client_id, false);
        }
    }
}

fn add_snake_inputs(
    mut commands: Commands,
    client: Query<&LocalId, With<Client>>,
    snakes: Query<
        Entity,
        (
            With<Controlled>,
            With<TailPoints>,
            With<SnakeInput>,
            Without<InputMarker<SnakeInput>>,
        ),
    >,
    actions: Query<&ActionOf<SnakeInput>, With<Action<MoveSnake>>>,
) {
    let Ok(client_id) = client.single().map(|id| id.0) else {
        return;
    };
    for snake in &snakes {
        commands
            .entity(snake)
            .insert(InputMarker::<SnakeInput>::default());
        if !actions.iter().any(|action| action.get() == snake) {
            spawn_snake_input_actions(&mut commands, snake, client_id, false);
        }
    }
}
