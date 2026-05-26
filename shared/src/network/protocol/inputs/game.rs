use bevy::prelude::*;
use lightyear::prelude::input::bei::InputAction;
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Reflect, Clone, Debug, PartialEq)]
pub struct PlayerInput;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct SpawnPlayer;
