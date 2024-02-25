use bevy::prelude::{Component, Reflect};
use lightyear::prelude::Message;
use serde::{Deserialize, Serialize};

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
pub struct FoodMarker;