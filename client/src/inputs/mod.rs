//! Handle inputs that are not networked, for example controlling local UI.

use bevy::prelude::*;
use lightyear::prelude::input::bei::{
    bindings, Action, ActionOf, EnhancedInputPlugin, InputAction, InputContextAppExt,
};

#[derive(Component, Debug, PartialEq, Eq, Clone, Copy, Reflect)]
pub struct LocalInputContext;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct ToggleCamera;

pub struct LocalInputsPlugin;

impl Plugin for LocalInputsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EnhancedInputPlugin>() {
            app.add_plugins(EnhancedInputPlugin);
        }
        app.add_input_context::<LocalInputContext>();
        app.add_systems(Startup, spawn_local_inputs);
        app.register_type::<LocalInputContext>();
    }
}

fn spawn_local_inputs(mut commands: Commands) {
    let context = commands.spawn(LocalInputContext).id();
    commands.spawn((
        ActionOf::<LocalInputContext>::new(context),
        Action::<ToggleCamera>::new(),
        bindings![KeyCode::KeyT],
    ));
}
