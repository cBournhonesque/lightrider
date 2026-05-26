use bevy::prelude::*;

mod collider;
mod death;

pub struct CollisionPlugin;

impl Plugin for CollisionPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(collider::ColliderPlugin);
        app.add_plugins(death::DeathPlugin);
    }
}
