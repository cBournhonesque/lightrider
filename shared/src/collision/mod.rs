use bevy::prelude::*;

pub mod collider;
pub mod layers;
pub struct CollisionPlugin;

impl Plugin for CollisionPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(collider::ColliderPlugin);
    }
}