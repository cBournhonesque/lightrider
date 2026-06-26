use bevy::app::{App, Plugin};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use lightyear::prelude::input::bei::{Action, ActionOf, InputMarker};
use lightyear::prelude::{
    Client, Connected, Controlled, ControlledBy, LocalId, MessageSender, PeerId, Predicted,
};

use crate::collision::death::DeathView;
use shared::network::protocol::prelude::*;

pub struct NetworkInputsPlugin;

#[derive(Resource)]
pub(crate) struct AutoRespawnRequests;

impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (send_player_spawn_requests, ensure_snake_inputs));
    }
}

fn send_player_spawn_requests(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    death_view: Option<Res<DeathView>>,
    auto_respawn: Option<Res<AutoRespawnRequests>>,
    mut next_auto_request_at: Local<f64>,
    mut clients: Query<&mut MessageSender<PlayerSpawnRequest>, (With<Client>, With<Connected>)>,
    players: Query<(&Player, &PlayerStatus), Or<(With<Controlled>, With<Predicted>)>>,
) {
    let respawn_ready = can_respawn_from_death_view(death_view.as_deref(), time.elapsed_secs());
    let wants_keyboard_respawn = respawn_ready
        && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter));
    let wants_auto_respawn =
        respawn_ready && auto_respawn.is_some() && time.elapsed_secs_f64() >= *next_auto_request_at;
    if !wants_keyboard_respawn && !wants_auto_respawn {
        return;
    }

    if !players
        .iter()
        .any(|(player, status)| player.snake.is_none() || *status == PlayerStatus::Dead)
    {
        return;
    }
    let Ok(mut sender) = clients.single_mut() else {
        return;
    };
    sender.send::<GameChannel>(PlayerSpawnRequest);

    if wants_auto_respawn {
        *next_auto_request_at = time.elapsed_secs_f64() + 0.25;
    }
}

fn can_respawn_from_death_view(death_view: Option<&DeathView>, now_seconds: f32) -> bool {
    death_view.is_none_or(|death_view| {
        death_view.stats.is_none() || now_seconds >= death_view.respawn_allowed_at_seconds
    })
}

fn ensure_snake_inputs(
    mut commands: Commands,
    client: Query<(Entity, &LocalId), With<Client>>,
    snakes: Query<
        (Entity, Has<InputMarker<SnakeInput>>, Option<&ControlledBy>),
        (
            With<TailPoints>,
            With<SnakeInput>,
            Or<(With<Controlled>, With<Predicted>)>,
        ),
    >,
    actions: Query<&ActionOf<SnakeInput>, (With<Action<MoveSnake>>, With<InputMarker<SnakeInput>>)>,
) {
    let Ok((client_entity, client_id)) = client.single() else {
        return;
    };
    for (snake, has_input_marker, controlled_by) in &snakes {
        add_snake_inputs(
            &mut commands,
            snake,
            client_id.0,
            client_entity,
            has_input_marker,
            controlled_by,
            &actions,
        );
    }
}

fn add_snake_inputs(
    commands: &mut Commands,
    snake: Entity,
    client_id: PeerId,
    client_entity: Entity,
    has_input_marker: bool,
    controlled_by: Option<&ControlledBy>,
    actions: &Query<
        &ActionOf<SnakeInput>,
        (With<Action<MoveSnake>>, With<InputMarker<SnakeInput>>),
    >,
) {
    if let Some(controlled_by) = controlled_by {
        if controlled_by.owner != client_entity {
            return;
        }
    }

    if !has_input_marker {
        commands
            .entity(snake)
            .insert(InputMarker::<SnakeInput>::default());
    }
    if !actions.iter().any(|action| action.get() == snake) {
        spawn_snake_input_actions(commands, snake, client_id, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn death_view(ready_at: f32) -> DeathView {
        DeathView {
            respawn_allowed_at_seconds: ready_at,
            stats: Some(PlayerDeathStats::default()),
            ..default()
        }
    }

    #[test]
    fn respawn_gate_waits_for_death_view_cooldown() {
        let view = death_view(3.0);

        assert!(!can_respawn_from_death_view(Some(&view), 2.99));
        assert!(can_respawn_from_death_view(Some(&view), 3.0));
    }

    #[test]
    fn respawn_gate_allows_initial_spawn_without_death_view() {
        assert!(can_respawn_from_death_view(None, 0.0));
    }
}
