use bevy::prelude::{Component, Reflect, Vec2};
use derive_more::{Add, Mul};
use lightyear::prelude::Message;
use serde::{Deserialize, Serialize};

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect, Add, Mul)]
pub struct Position(pub Vec2);