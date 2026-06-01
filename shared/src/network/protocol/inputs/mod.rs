use bevy::prelude::*;
use lightyear::prelude::input::bei::{Action, ActionOf, Bindings, Cardinal, InputMarker};
use lightyear::prelude::{PeerId, PreSpawned};

pub use movement::{MoveSnake, SnakeInput};

mod movement;

#[derive(Component)]
pub struct ServerAction;

fn action_prespawn_hash(client_id: PeerId, salt: u64) -> u64 {
    client_id
        .to_bits()
        .wrapping_mul(6364136223846793005)
        .wrapping_add(salt)
}

fn action_prespawn(client_id: PeerId, salt: u64, context: Entity, is_server: bool) -> PreSpawned {
    let prespawned = PreSpawned::new(action_prespawn_hash(client_id, salt));
    if is_server {
        prespawned
    } else {
        prespawned.for_receiver(context)
    }
}

fn configure_action_entity(
    action: &mut EntityCommands,
    _client_id: PeerId,
    is_server: bool,
    input_marker: impl Bundle,
) {
    if is_server {
        action.insert(ServerAction);
    } else {
        action.insert(input_marker);
    }
}

pub fn spawn_snake_input_actions(
    commands: &mut Commands,
    snake_entity: Entity,
    client_id: PeerId,
    is_server: bool,
) {
    let mut wasd = commands.spawn((
        ActionOf::<SnakeInput>::new(snake_entity),
        Action::<MoveSnake>::new(),
        Bindings::spawn(Cardinal::wasd_keys()),
        action_prespawn(client_id, 10, snake_entity, is_server),
    ));
    configure_action_entity(
        &mut wasd,
        client_id,
        is_server,
        InputMarker::<SnakeInput>::default(),
    );

    let mut arrows = commands.spawn((
        ActionOf::<SnakeInput>::new(snake_entity),
        Action::<MoveSnake>::new(),
        Bindings::spawn(Cardinal::arrows()),
        action_prespawn(client_id, 11, snake_entity, is_server),
    ));
    configure_action_entity(
        &mut arrows,
        client_id,
        is_server,
        InputMarker::<SnakeInput>::default(),
    );
}
