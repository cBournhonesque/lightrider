use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::{Client, MessageReceiver, MessageSender, PredictionMetrics};
use lightyear_tools::ui::debug::{DebugUIPlugin, MetricsPanelSettings};
use shared::network::protocol::prelude::*;

use crate::render::ui_style;

const ADMIN_PANEL_WIDTH: f32 = 260.0;
const MAX_PASSWORD_CHARS: usize = 96;

#[derive(Resource, Debug, Default)]
struct AdminUiState {
    unlock_visible: bool,
    panel_visible: bool,
    unlocked: bool,
    password: String,
    status: String,
    last_status: AdminStatus,
    desired_bots: u16,
}

#[derive(Component)]
struct AdminUnlockRoot;

#[derive(Component)]
struct AdminPanelRoot;

#[derive(Component)]
struct AdminPasswordText;

#[derive(Component)]
struct AdminStatusText;

#[derive(Component)]
struct AdminBotText;

#[derive(Component)]
struct AdminStatsText;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum AdminButton {
    SubmitPassword,
    DecrementBots,
    IncrementBots,
    ApplyBots,
    Close,
}

pub(crate) struct ClientAdminPlugin;

impl Plugin for ClientAdminPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AdminUiState>();
        app.insert_resource(MetricsPanelSettings {
            enabled: false,
            window_len: 50,
            alpha: 0.62,
        });
        app.add_plugins(DebugUIPlugin);
        app.add_systems(Startup, spawn_admin_ui);
        app.add_systems(
            Update,
            (
                toggle_admin_shortcut,
                admin_password_input,
                handle_admin_buttons,
                receive_admin_responses,
                update_admin_view,
            )
                .chain(),
        );
    }
}

fn spawn_admin_ui(mut commands: Commands) {
    let title_font = TextFont {
        font_size: 16.0,
        ..default()
    };
    let body_font = TextFont {
        font_size: 13.0,
        ..default()
    };

    commands
        .spawn((
            AdminUnlockRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(ADMIN_PANEL_WIDTH),
                margin: UiRect {
                    left: Val::Px(-ADMIN_PANEL_WIDTH * 0.5),
                    top: Val::Px(-86.0),
                    ..default()
                },
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            ui_style::panel_background(0.72),
            ui_style::panel_border(),
            ui_style::panel_shadow(),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Admin"),
                ui_style::title_color(),
                ui_style::text_glow(),
                title_font.clone(),
            ));
            parent.spawn((
                AdminPasswordText,
                Text::new("Password: "),
                ui_style::body_color(),
                ui_style::text_glow(),
                body_font.clone(),
            ));
            parent.spawn((
                AdminStatusText,
                Text::new("Enter password, then press Enter."),
                ui_style::body_color(),
                ui_style::text_glow(),
                body_font.clone(),
            ));
            parent
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                },))
                .with_children(|row| {
                    admin_button(row, AdminButton::SubmitPassword, "UNLOCK", 86.0);
                    admin_button(row, AdminButton::Close, "CLOSE", 72.0);
                });
        });

    commands
        .spawn((
            AdminPanelRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(58.0),
                width: Val::Px(ADMIN_PANEL_WIDTH),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            ui_style::panel_background(0.62),
            ui_style::panel_border(),
            ui_style::panel_shadow(),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Admin"),
                ui_style::title_color(),
                ui_style::text_glow(),
                title_font,
            ));
            parent.spawn((
                AdminBotText,
                Text::new("Bots: --"),
                ui_style::body_color(),
                ui_style::text_glow(),
                body_font.clone(),
            ));
            parent
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    ..default()
                },))
                .with_children(|row| {
                    admin_button(row, AdminButton::DecrementBots, "-", 36.0);
                    admin_button(row, AdminButton::IncrementBots, "+", 36.0);
                    admin_button(row, AdminButton::ApplyBots, "SET", 58.0);
                    admin_button(row, AdminButton::Close, "HIDE", 58.0);
                });
            parent.spawn((
                AdminStatsText,
                Text::new("Rollbacks: --"),
                ui_style::body_color(),
                ui_style::text_glow(),
                body_font,
            ));
        });
}

fn admin_button(parent: &mut ChildSpawnerCommands, action: AdminButton, label: &str, width: f32) {
    parent
        .spawn((
            Button,
            action,
            Node {
                width: Val::Px(width),
                height: Val::Px(28.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                ..default()
            },
            BackgroundColor(admin_button_color(Interaction::None)),
            ui_style::button_border(),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                ui_style::title_color(),
                ui_style::text_glow(),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
            ));
        });
}

fn toggle_admin_shortcut(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<AdminUiState>) {
    let modifier = (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
        && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
        && (keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight));
    if modifier && keys.just_pressed(KeyCode::KeyA) {
        if state.unlocked {
            state.panel_visible = !state.panel_visible;
        } else {
            state.unlock_visible = !state.unlock_visible;
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.unlock_visible = false;
        state.panel_visible = false;
    }
}

fn admin_password_input(
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<AdminUiState>,
    mut clients: Query<&mut MessageSender<AdminLoginRequest>, (With<Client>, With<Connected>)>,
) {
    if !state.unlock_visible || state.unlocked {
        keyboard.clear();
        return;
    }

    let shortcut_modifier = (keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight))
        && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
        && (keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight));
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }
        match event.key_code {
            KeyCode::Enter | KeyCode::NumpadEnter => {
                send_admin_login(&mut clients, &mut state);
            }
            KeyCode::Backspace => {
                state.password.pop();
            }
            _ if shortcut_modifier => {}
            _ => {
                if let Some(text) = &event.text {
                    for character in text.chars().filter(|character| !character.is_control()) {
                        if state.password.len() < MAX_PASSWORD_CHARS {
                            state.password.push(character);
                        }
                    }
                }
            }
        }
    }
}

fn handle_admin_buttons(
    mut state: ResMut<AdminUiState>,
    mut buttons: Query<
        (&Interaction, &AdminButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut login_senders: Query<
        &mut MessageSender<AdminLoginRequest>,
        (With<Client>, With<Connected>),
    >,
    mut command_senders: Query<&mut MessageSender<AdminCommand>, (With<Client>, With<Connected>)>,
) {
    for (interaction, action, mut color) in &mut buttons {
        if *interaction == Interaction::Pressed {
            match action {
                AdminButton::SubmitPassword => send_admin_login(&mut login_senders, &mut state),
                AdminButton::DecrementBots => {
                    state.desired_bots = state.desired_bots.saturating_sub(1);
                }
                AdminButton::IncrementBots => {
                    state.desired_bots = state.desired_bots.saturating_add(1);
                }
                AdminButton::ApplyBots => {
                    if let Ok(mut sender) = command_senders.single_mut() {
                        sender.send::<GameChannel>(AdminCommand::SetNumBots {
                            count: state.desired_bots,
                        });
                        state.status = format!("requested {} bots", state.desired_bots);
                    }
                }
                AdminButton::Close => {
                    state.unlock_visible = false;
                    state.panel_visible = false;
                }
            }
        }
        *color = BackgroundColor(admin_button_color(*interaction));
    }
}

fn send_admin_login(
    clients: &mut Query<&mut MessageSender<AdminLoginRequest>, (With<Client>, With<Connected>)>,
    state: &mut AdminUiState,
) {
    if state.password.is_empty() {
        state.status = "enter password first".to_string();
        return;
    }
    let Ok(mut sender) = clients.single_mut() else {
        state.status = "not connected".to_string();
        return;
    };
    sender.send::<GameChannel>(AdminLoginRequest {
        password: state.password.clone(),
    });
    state.status = "checking password".to_string();
}

fn receive_admin_responses(
    mut state: ResMut<AdminUiState>,
    mut clients: Query<&mut MessageReceiver<AdminResponse>, (With<Client>, With<Connected>)>,
) {
    let Ok(mut receiver) = clients.single_mut() else {
        return;
    };
    for response in receiver.receive() {
        state.status = response.message;
        state.last_status = response.status;
        state.desired_bots = response.status.target_bot_count;
        state.unlocked = response.status.authenticated;
        if response.status.authenticated {
            state.password.clear();
            state.unlock_visible = false;
            state.panel_visible = true;
        }
    }
}

fn update_admin_view(
    state: Res<AdminUiState>,
    metrics: Option<Res<PredictionMetrics>>,
    mut debug_panel: Option<ResMut<MetricsPanelSettings>>,
    mut unlock_roots: Query<&mut Visibility, (With<AdminUnlockRoot>, Without<AdminPanelRoot>)>,
    mut panel_roots: Query<&mut Visibility, (With<AdminPanelRoot>, Without<AdminUnlockRoot>)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<AdminPasswordText>>,
        Query<&mut Text, With<AdminStatusText>>,
        Query<&mut Text, With<AdminBotText>>,
        Query<&mut Text, With<AdminStatsText>>,
    )>,
) {
    if let Ok(mut visibility) = unlock_roots.single_mut() {
        *visibility = if state.unlock_visible && !state.unlocked {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visibility) = panel_roots.single_mut() {
        *visibility = if state.panel_visible && state.unlocked {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Some(settings) = debug_panel.as_mut() {
        let enabled = state.panel_visible && state.unlocked;
        if settings.enabled != enabled {
            settings.enabled = enabled;
        }
    }
    if let Ok(mut text) = texts.p0().single_mut() {
        text.0 = format!("Password: {}", "*".repeat(state.password.chars().count()));
    }
    if let Ok(mut text) = texts.p1().single_mut() {
        text.0 = state.status.clone();
    }
    if let Ok(mut text) = texts.p2().single_mut() {
        let room = state
            .last_status
            .room
            .map(|room| room.0.to_string())
            .unwrap_or_else(|| "--".to_string());
        text.0 = format!(
            "Room: {room}\nBots: {} / {}\nDesired: {}",
            state.last_status.bot_count, state.last_status.target_bot_count, state.desired_bots
        );
    }
    if let Ok(mut text) = texts.p3().single_mut() {
        text.0 = format_admin_stats(metrics.as_deref());
    }
}

fn format_admin_stats(metrics: Option<&PredictionMetrics>) -> String {
    let Some(metrics) = metrics else {
        return "Rollbacks: --\nRollback ticks: --\nAvg rollback: --".to_string();
    };
    let average_depth = if metrics.rollbacks == 0 {
        0.0
    } else {
        metrics.rollback_ticks as f32 / metrics.rollbacks as f32
    };
    format!(
        "Rollbacks: {}\nRollback ticks: {}\nAvg rollback: {:.1}",
        metrics.rollbacks, metrics.rollback_ticks, average_depth
    )
}

fn admin_button_color(interaction: Interaction) -> Color {
    ui_style::button_background(false, interaction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_stats_formats_prediction_metrics() {
        let metrics = PredictionMetrics {
            rollbacks: 3,
            rollback_ticks: 12,
        };

        let text = format_admin_stats(Some(&metrics));

        assert!(text.contains("Rollbacks: 3"));
        assert!(text.contains("Rollback ticks: 12"));
        assert!(text.contains("Avg rollback: 4.0"));
    }

    #[test]
    fn admin_view_system_runs_without_query_conflicts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<AdminUiState>();
        app.add_systems(Update, update_admin_view);

        app.world_mut().spawn((AdminUnlockRoot, Visibility::Hidden));
        app.world_mut().spawn((AdminPanelRoot, Visibility::Hidden));
        app.world_mut().spawn((AdminPasswordText, Text::new("")));
        app.world_mut().spawn((AdminStatusText, Text::new("")));
        app.world_mut().spawn((AdminBotText, Text::new("")));
        app.world_mut().spawn((AdminStatsText, Text::new("")));

        app.update();
    }
}
