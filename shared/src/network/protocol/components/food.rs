use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};

use crate::colors::SnakePaletteColor;

#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
pub struct FoodMarker;

#[derive(Component, Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct FoodColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl FoodColor {
    pub fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }
}

impl From<SnakePaletteColor> for FoodColor {
    fn from(color: SnakePaletteColor) -> Self {
        Self::new(color.red, color.green, color.blue)
    }
}
