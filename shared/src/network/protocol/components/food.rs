use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};

#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
pub struct FoodMarker;
