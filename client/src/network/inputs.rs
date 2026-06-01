use bevy::app::{App, Plugin};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use lightyear::prelude::input::bei::{Action, ActionOf, InputMarker};
use lightyear::prelude::{
    Client, Connected, Controlled, InputTimeline, IsSynced, LocalId, MessageSender,
};

use shared::network::protocol::prelude::*;

pub struct NetworkInputsPlugin;

#[derive(Resource)]
pub(crate) struct AutoRespawnRequests;

impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (send_player_spawn_requests, add_snake_inputs));
    }
}

fn send_player_spawn_requests(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    auto_respawn: Option<Res<AutoRespawnRequests>>,
    mut next_auto_request_at: Local<f64>,
    mut clients: Query<
        &mut MessageSender<PlayerSpawnRequest>,
        (With<Client>, With<Connected>, With<IsSynced<InputTimeline>>),
    >,
    players: Query<(&Player, &PlayerStatus), With<Controlled>>,
) {
    let wants_keyboard_respawn =
        keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
    let wants_auto_respawn =
        auto_respawn.is_some() && time.elapsed_secs_f64() >= *next_auto_request_at;
    if !wants_keyboard_respawn && !wants_auto_respawn {
        return;
    }

    if !players
        .iter()
        .any(|(player, status)| player.snake.is_none() || *status == PlayerStatus::Dead)
    {
        return;
    }
    let Ok(mut sender) = clients.single_mut() else {
        return;
    };
    sender.send::<GameChannel>(PlayerSpawnRequest);

    if wants_auto_respawn {
        *next_auto_request_at = time.elapsed_secs_f64() + 0.25;
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
