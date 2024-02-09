use bevy::prelude::*;
use bevy_xpbd_2d::PhysicsStepSet;
use bevy_xpbd_2d::plugins::PhysicsSetupPlugin;
use bevy_xpbd_2d::prelude::SpatialQueryPlugin;

mod collider;
pub(crate) mod layers;
mod death;

pub struct CollisionPlugin;


#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum MainSet {
    // update colliders
    UpdateColliders,
    // compute all collision information
    ComputeCollision,
}

impl Plugin for CollisionPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(PhysicsSetupPlugin::new(Update));
        app.add_plugins(SpatialQueryPlugin::new(Update));
        // events
        app.add_event::<collider::SnakeCollisionEvent>();
        app.add_event::<collider::SnakeFrictionEvent>();
        // sets
        app.configure_sets(Update, (MainSet::UpdateColliders, PhysicsStepSet::SpatialQuery, MainSet::ComputeCollision).chain());
        // systems
        app.add_systems(Update, (
            collider::update_collider.in_set(MainSet::UpdateColliders),
            (collider::snake_collisions, collider::snake_friction)
                .in_set(MainSet::ComputeCollision),
            death::handle_collision.after(MainSet::ComputeCollision)
        ));
    }
}