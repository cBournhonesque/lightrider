//! Handle inputs that are not networked, for example controlling local UI.

use bevy::prelude::*;
use lightyear::prelude::input::bei::{
    bindings, Action, ActionOf, EnhancedInputPlugin, InputAction, InputContextAppExt,
};

use crate::render::ui_style;

#[derive(Component, Debug, PartialEq, Eq, Clone, Copy, Reflect)]
pub struct LocalInputContext;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct ToggleCamera;

#[derive(Resource, Clone, Copy, Debug)]
struct LocalInputSettings {
    debug_enabled: bool,
}

#[derive(Component)]
struct ShortcutHelpRoot;

pub struct LocalInputsPlugin {
    pub(crate) debug_enabled: bool,
}

impl Plugin for LocalInputsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EnhancedInputPlugin>() {
            app.add_plugins(EnhancedInputPlugin);
        }
        app.insert_resource(LocalInputSettings {
            debug_enabled: self.debug_enabled,
        });
        app.add_input_context::<LocalInputContext>();
        app.add_systems(Startup, spawn_local_inputs);
        app.add_systems(Update, toggle_shortcut_help);
        app.register_type::<LocalInputContext>();
    }
}

fn spawn_local_inputs(mut commands: Commands, settings: Res<LocalInputSettings>) {
    if settings.debug_enabled {
        let context = commands.spawn(LocalInputContext).id();
        commands.spawn((
            ActionOf::<LocalInputContext>::new(context),
            Action::<ToggleCamera>::new(),
            bindings![KeyCode::KeyT],
        ));
        spawn_shortcut_help(&mut commands);
    }
}

fn spawn_shortcut_help(commands: &mut Commands) {
    commands
        .spawn((
            ShortcutHelpRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(12.0),
                width: Val::Px(290.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            ui_style::panel_background(0.68),
            ui_style::panel_border(),
            ui_style::panel_shadow(),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(
                    "Shortcuts\nWASD / Arrows: turn\nEnter / Space: respawn\nT: debug camera\n?: toggle this help",
                ),
                ui_style::body_color(),
                ui_style::text_glow(),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
            ));
        });
}

fn toggle_shortcut_help(
    settings: Res<LocalInputSettings>,
    keys: Res<ButtonInput<KeyCode>>,
    mut help: Query<&mut Visibility, With<ShortcutHelpRoot>>,
) {
    if !settings.debug_enabled
        || !keys.just_pressed(KeyCode::Slash)
        || !(keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
    {
        return;
    }
    if let Ok(mut visibility) = help.single_mut() {
        *visibility = match *visibility {
            Visibility::Hidden => Visibility::Inherited,
            _ => Visibility::Hidden,
        };
    }
}
