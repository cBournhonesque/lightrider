use bevy::prelude::*;
use lightyear::prelude::input::bei::InputAction;
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Reflect, Clone, Debug, PartialEq)]
pub struct SnakeInput;

#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub struct MoveSnake;
