use bevy_xpbd_2d::prelude::PhysicsLayer;

#[derive(PhysicsLayer)]
pub(crate) enum CollideLayer {
    Player,
    Food,
    Wall
}