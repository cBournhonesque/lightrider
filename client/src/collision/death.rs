//! This module handles:
//! - the player's death
//! - switching from the game state to the dead state
//! - respawning logic
use bevy::app::{App, Plugin};
use bevy::prelude::*;
use lightyear::prelude::{Client, Controlled, MessageReceiver, Predicted};
use shared::config::GameConfig;
use shared::network::protocol::prelude::{
    DeathReason, HasPlayer, Player, PlayerDeath, PlayerDeathStats, TailPoints,
};

pub(crate) struct DeathPlugin;

#[derive(Message, Clone, Debug, PartialEq)]
pub(crate) struct ConfirmedDeath {
    pub(crate) message: PlayerDeath,
    pub(crate) local_player: bool,
    pub(crate) position: Option<Vec2>,
    pub(crate) tail: Option<TailPoints>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
struct LastLocalSnakeTail(Option<TailPoints>);

#[derive(Resource, Clone, Debug, Default, PartialEq, Reflect)]
pub(crate) struct DeathView {
    pub(crate) killer_snake: Option<Entity>,
    pub(crate) respawn_allowed_at_seconds: f32,
    pub(crate) message: String,
    pub(crate) stats: Option<PlayerDeathStats>,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States, Reflect)]
enum GameState {
    Dead,
    #[default]
    Alive,
}

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {
        // states
        app.init_state::<GameState>();
        app.init_resource::<DeathView>();
        app.add_message::<ConfirmedDeath>();

        // systems
        // TODO: toggling the actions is not enough, ideally we would disable/enable the entire input plugin
        // dead
        app.add_systems(OnEnter(GameState::Dead), enable_dead_actions);
        app.add_systems(Update, set_alive_state.run_if(in_state(GameState::Dead)));

        // alive
        app.add_systems(
            OnEnter(GameState::Alive),
            (enable_alive_actions, clear_death_view),
        );

        // all
        app.init_resource::<LastLocalSnakeTail>();
        app.add_systems(
            Update,
            (cache_local_snake_tail, handle_death_message).chain(),
        );

        // reflect
        app.register_type::<GameState>();
        app.register_type::<DeathView>();
    }
}

// 1. if it's our own death, enter death state
// 2. if it's someone else's death, play death animation
fn handle_death_message(
    mut next_state: ResMut<NextState<GameState>>,
    mut death_view: ResMut<DeathView>,
    config: Res<GameConfig>,
    time: Res<Time>,
    mut receivers: Query<&mut MessageReceiver<PlayerDeath>, With<Client>>,
    player: Query<(Entity, &Player), With<Controlled>>,
    tails: Query<&TailPoints>,
    local_tail_cache: Res<LastLocalSnakeTail>,
    mut confirmed_deaths: MessageWriter<ConfirmedDeath>,
) {
    let Ok(mut receiver) = receivers.single_mut() else {
        return;
    };
    let local_player = player.single().ok();
    for message in receiver.receive() {
        trace!(?message, "Received death message");
        let local_player_died =
            local_player.map(|(entity, _)| entity) == Some(message.killed_player);
        confirmed_deaths.write(ConfirmedDeath {
            position: death_sound_position(&message, &tails),
            tail: death_tail_snapshot(&message, &tails, local_player, &local_tail_cache),
            message: message.clone(),
            local_player: local_player_died,
        });
        if local_player_died {
            debug!("I died");
            death_view.killer_snake = death_camera_target(&message);
            death_view.respawn_allowed_at_seconds =
                time.elapsed_secs() + config.respawn.player_cooldown_seconds.max(0.0);
            death_view.message = death_message(&message);
            death_view.stats = Some(message.stats);
            next_state.set(GameState::Dead);
        }
    }
}

fn cache_local_snake_tail(
    player: Query<&Player, With<Controlled>>,
    tails: Query<&TailPoints>,
    mut cache: ResMut<LastLocalSnakeTail>,
) {
    let Some(tail) = player
        .single()
        .ok()
        .and_then(|player| player.snake)
        .and_then(|snake| tails.get(snake).ok())
    else {
        return;
    };
    cache.0 = Some(tail.clone());
}

fn death_tail_snapshot(
    message: &PlayerDeath,
    tails: &Query<&TailPoints>,
    local_player: Option<(Entity, &Player)>,
    local_tail_cache: &LastLocalSnakeTail,
) -> Option<TailPoints> {
    tails.get(message.killed_snake).ok().cloned().or_else(|| {
        let (player_entity, player) = local_player?;
        if player_entity != message.killed_player {
            return None;
        }
        player
            .snake
            .and_then(|snake| tails.get(snake).ok().cloned())
            .or_else(|| local_tail_cache.0.clone())
    })
}

fn death_sound_position(message: &PlayerDeath, tails: &Query<&TailPoints>) -> Option<Vec2> {
    tails
        .get(message.killed_snake)
        .or_else(|_| tails.get(message.killer_snake))
        .ok()
        .map(|tail| tail.front().0)
}

fn death_camera_target(message: &PlayerDeath) -> Option<Entity> {
    match message.reason {
        DeathReason::Collision
            if message.killer_player != message.killed_player
                && message.killer_snake != message.killed_snake =>
        {
            Some(message.killer_snake)
        }
        DeathReason::Collision | DeathReason::Boundary | DeathReason::Suicide => None,
    }
}

fn death_message(message: &PlayerDeath) -> String {
    match message.reason {
        DeathReason::Collision
            if message.killer_player != message.killed_player
                && message.killer_snake != message.killed_snake =>
        {
            format!("Killed by {}", message.killer_name)
        }
        DeathReason::Boundary => "Out of bounds".to_string(),
        DeathReason::Suicide | DeathReason::Collision => "You died".to_string(),
    }
}

// During dead state, show the death screen to the user
#[allow(dead_code)]
fn show_death_screen(_commands: Commands) {
    // commands.spawn(NodeBundle {
    //     style: Style {
    //         display: Display::Flex,
    //         position_type: PositionType::Absolute,
    //         flex_direction: FlexDirection::Column,
    //         justify_content: JustifyContent::Center,
    //         align_items: AlignItems::Center,
    //         ..Default::default()
    //     },
    //     ..Default::default()
    // })
    // })
}

// 1. press spawn, send message to server
// 2. server
fn enable_dead_actions() {
    trace!("Enable dead actions");
}

fn enable_alive_actions() {
    trace!("Enable alive actions");
}

fn clear_death_view(mut death_view: ResMut<DeathView>) {
    *death_view = DeathView::default();
}

// TODO: receive an event instead
/// When we receive a new predicted snake from the server, that means we respawn!
/// Switch the game state
fn set_alive_state(
    mut next_state: ResMut<NextState<GameState>>,
    my_snake: Query<Entity, (Added<HasPlayer>, With<Predicted>)>,
) {
    if my_snake.iter().next().is_some() {
        trace!("Setting state to Alive");
        next_state.set(GameState::Alive);
    }
}
