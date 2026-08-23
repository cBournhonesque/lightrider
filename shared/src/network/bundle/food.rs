use crate::network::protocol::prelude::*;
use bevy::prelude::*;

#[derive(Bundle)]
pub struct FoodBundle {
    pub position: Position,
    pub marker: FoodMarker,
    pub room: RoomId,
}

impl FoodBundle {
    pub fn new(position: Position) -> Self {
        Self::new_in_room(position, RoomId::default())
    }

    pub fn new_in_room(position: Position, room: RoomId) -> Self {
        Self {
            position,
            marker: FoodMarker,
            room,
        }
    }
}
