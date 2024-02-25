use bevy::prelude::*;
use bevy_xpbd_2d::components::{CollisionLayers, Rotation};
use bevy_xpbd_2d::prelude::Collider;
use crate::collision::layers::CollideLayer;
use crate::network::protocol::prelude::*;

pub const FOOD_COLLISION_RADIUS: f32 = 20.0;

#[derive(Bundle)]
pub struct FoodBundle {
    pub position: Position,
    pub marker: FoodMarker,
    // NOTE: position/rotation are necessary for spatial queries (to compute an isometry). Otherwise we don't really use them
    //  so let's leave them at default
    pub _position: bevy_xpbd_2d::components::Position,
    pub _rotation: Rotation,
    pub collider: Collider,
    pub collider_layers: CollisionLayers,
}

impl FoodBundle {
    pub fn new(position: Position) -> Self {
        // NOTE: bevy_xpbd uses position to do ray casts! The collider just provides the shape of the object
        //  for polyline it's different because the polyline contains the vertices of the object directly
        let _position = bevy_xpbd_2d::components::Position::from_xy(position.0.x, position.0.y);
        Self {
            position,
            marker: FoodMarker,
            _position,
            _rotation: Rotation::default(),
            collider: Collider::circle(FOOD_COLLISION_RADIUS),
            collider_layers: CollisionLayers::new([CollideLayer::Food], [CollideLayer::Player]),
        }
    }
}
