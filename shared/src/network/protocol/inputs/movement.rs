use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use lightyear::prelude::LeafwingUserAction;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Hash, Reflect, Actionlike)]
pub enum PlayerMovement {
    Up,
    Down,
    Left,
    Right,
}

impl LeafwingUserAction for PlayerMovement {}