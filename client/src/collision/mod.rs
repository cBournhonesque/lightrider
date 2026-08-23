use bevy::prelude::*;

pub(crate) mod death;

pub struct CollisionPlugin;

impl Plugin for CollisionPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(death::DeathPlugin);
    }
}
