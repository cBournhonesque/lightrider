use bevy_xpbd_2d::prelude::PhysicsLayer;

/// Different layers for collision
#[derive(PhysicsLayer)]
pub enum CollideLayer {
    Player,
    Food,
    Wall
}