use bevy::prelude::{Component, Reflect, Vec2};
use derive_more::{Add, Mul};
use serde::{Deserialize, Serialize};

#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
pub struct Position(pub Vec2);

pub fn interpolate_position(start: Position, end: Position, t: f32) -> Position {
    Position(start.0.lerp(end.0, t.clamp(0.0, 1.0)))
}

#[derive(
    Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect,
)]
pub struct RoomId(pub u64);
