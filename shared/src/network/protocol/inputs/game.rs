use bevy::prelude::Reflect;
use leafwing_input_manager::Actionlike;
use lightyear::prelude::LeafwingUserAction;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Hash, Reflect, Actionlike)]
pub enum DeadGameAction {
    Spawn,
}

impl LeafwingUserAction for DeadGameAction {}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Hash, Reflect, Actionlike)]
pub enum AliveGameAction {
    ToggleCamera,
}

impl LeafwingUserAction for AliveGameAction {}