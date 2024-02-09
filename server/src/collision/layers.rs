use bevy_xpbd_2d::prelude::PhysicsLayer;

/// Different layers for collision
#[derive(PhysicsLayer)]
pub(crate) enum CollideLayer {
    Player,
    Food,
    Wall
}