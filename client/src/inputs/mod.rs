//! Handle inputs that are not networked (for example controlling the UI)


use bevy::prelude::*;
use leafwing_input_manager::Actionlike;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Reflect, Actionlike)]
pub enum LocalInput {
    ToggleCamera,
}

pub struct LocalInputsPlugin;

impl Plugin for LocalInputsPlugin {
    fn build(&self, app: &mut App) {
        // plugin
        app.add_plugins(leafwing_input_manager::prelude::InputManagerPlugin::<LocalInput>::default());

        // registry
        app.register_type::<LocalInput>();
    }
}