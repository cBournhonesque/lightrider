use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use lightyear::prelude::input::bei::{Action, ActionOf, Bindings, Cardinal, InputMarker};
use lightyear::prelude::{
    InterpolationTarget, NetworkTarget, PeerId, PreSpawned, PredictionTarget, Replicate,
};

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
    client_id: PeerId,
    is_server: bool,
    input_marker: impl Bundle,
) {
    if is_server {
        action.insert((
            ServerAction,
            Replicate::to_clients(NetworkTarget::Single(client_id)),
            PredictionTarget::manual(Vec::new()),
            InterpolationTarget::manual(Vec::new()),
        ));
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

pub(crate) fn cleanup_orphaned_snake_input_actions(
    mut commands: Commands,
    snakes: Query<(), With<SnakeInput>>,
    actions: Query<(Entity, &ActionOf<SnakeInput>), With<Action<MoveSnake>>>,
) {
    for (action, action_of) in &actions {
        if !snakes.contains(action_of.get()) {
            commands.entity(action).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn despawning_snake_cleans_up_input_actions() {
        let mut app = App::new();
        app.add_systems(Update, cleanup_orphaned_snake_input_actions);
        let snake = app.world_mut().spawn(SnakeInput).id();
        let action = app
            .world_mut()
            .spawn((
                ActionOf::<SnakeInput>::new(snake),
                Action::<MoveSnake>::new(),
            ))
            .id();

        app.world_mut().entity_mut(snake).despawn();
        app.update();

        assert!(app.world().get_entity(action).is_err());
    }
}
