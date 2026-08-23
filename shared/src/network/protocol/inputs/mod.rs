use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use lightyear::prelude::input::bei::{Action, ActionOf};
use lightyear::prelude::ReplicateLike;

pub use movement::{MoveSnake, SnakeInput};

mod movement;

#[derive(Component)]
pub struct ServerAction;

pub fn spawn_snake_input_actions(commands: &mut Commands, snake_entity: Entity) -> Entity {
    commands
        .spawn((
            ActionOf::<SnakeInput>::new(snake_entity),
            Action::<MoveSnake>::new(),
            ServerAction,
            ReplicateLike { root: snake_entity },
        ))
        .id()
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

    #[test]
    fn spawned_snake_input_action_replicates_like_snake_root() {
        fn spawn_action(mut commands: Commands, snakes: Query<Entity, With<SnakeInput>>) {
            let snake = snakes.single().unwrap();
            spawn_snake_input_actions(&mut commands, snake);
        }

        let mut app = App::new();
        app.add_systems(Update, spawn_action);
        let snake = app.world_mut().spawn(SnakeInput).id();

        app.update();

        let mut actions = app
            .world_mut()
            .query::<(Entity, &ActionOf<SnakeInput>, &ReplicateLike)>();
        let collected = actions.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(collected.len(), 1);
        let (_action, action_of, replicate_like) = collected[0];
        assert_eq!(action_of.get(), snake);
        assert_eq!(replicate_like.root, snake);
    }
}
