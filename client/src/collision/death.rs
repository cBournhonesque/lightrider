//! This module handles:
//! - the player's death
//! - switching from the game state to the dead state
//! - respawning logic
use bevy::app::{App, Plugin};
use bevy::prelude::*;
use lightyear::prelude::{Client, Controlled, MessageReceiver, Predicted};
use shared::config::GameConfig;
use shared::network::protocol::prelude::{
    DeathReason, HasPlayer, Player, PlayerDeath, PlayerDeathStats, PlayerScore, PlayerStats,
    PlayerStatus, SnakeHead, TailLength, TailPoints, TailPolyline,
};

pub(crate) struct DeathPlugin;

#[derive(Message, Clone, Debug, PartialEq)]
pub(crate) struct ConfirmedDeath {
    pub(crate) message: PlayerDeath,
    pub(crate) local_player: bool,
    pub(crate) position: Option<Vec2>,
    pub(crate) tail: Option<TailPolyline>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
struct LastLocalSnakeTail(Option<TailPolyline>);

#[derive(Resource, Clone, Debug, Default, PartialEq)]
struct HadLocalSnake(bool);

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
        app.init_resource::<HadLocalSnake>();
        app.add_systems(
            Update,
            (
                cache_local_snake_tail,
                handle_death_message,
                ensure_death_view_for_dead_local_player,
            )
                .chain(),
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
    tails: Query<(&SnakeHead, &TailPoints, Option<&TailLength>)>,
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
        let tail = death_tail_snapshot(&message, &tails, local_player, &local_tail_cache);
        confirmed_deaths.write(ConfirmedDeath {
            position: death_position(&message, &tails, tail.as_ref()),
            tail,
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

fn ensure_death_view_for_dead_local_player(
    mut next_state: ResMut<NextState<GameState>>,
    mut death_view: ResMut<DeathView>,
    mut had_local_snake: ResMut<HadLocalSnake>,
    config: Res<GameConfig>,
    time: Res<Time>,
    player: Query<
        (
            &Player,
            &PlayerStatus,
            Option<&PlayerScore>,
            Option<&PlayerStats>,
        ),
        With<Controlled>,
    >,
    predicted_snakes: Query<Entity, (With<Controlled>, With<Predicted>, With<SnakeHead>)>,
) {
    let Ok((player, status, score, stats)) = player.single() else {
        return;
    };

    let has_local_snake = player.snake.is_some() || predicted_snakes.iter().next().is_some();
    if has_local_snake {
        had_local_snake.0 = true;
        return;
    }

    if !had_local_snake.0 || death_view.stats.is_some() || *status != PlayerStatus::Dead {
        return;
    }

    debug!("Showing fallback death recap before detailed death message arrived");
    *death_view = fallback_death_view(
        time.elapsed_secs(),
        &config,
        score.map(|score| score.value),
        stats.copied(),
    );
    next_state.set(GameState::Dead);
}

fn fallback_death_view(
    now_seconds: f32,
    config: &GameConfig,
    score: Option<u32>,
    stats: Option<PlayerStats>,
) -> DeathView {
    DeathView {
        killer_snake: None,
        respawn_allowed_at_seconds: now_seconds + config.respawn.player_cooldown_seconds.max(0.0),
        message: "You died".to_string(),
        stats: Some(match stats {
            Some(stats) => PlayerDeathStats::from_live(score.unwrap_or_default(), &stats),
            None => PlayerDeathStats::default(),
        }),
    }
}

fn cache_local_snake_tail(
    local_snakes: Query<
        (&SnakeHead, &TailPoints, Option<&TailLength>),
        (With<Controlled>, With<Predicted>),
    >,
    player: Query<&Player, With<Controlled>>,
    tails: Query<(&SnakeHead, &TailPoints, Option<&TailLength>)>,
    mut cache: ResMut<LastLocalSnakeTail>,
) {
    if let Ok(tail) = local_snakes.single() {
        cache.0 = Some(visible_tail(tail.0, tail.1, tail.2));
        return;
    }

    let Some(tail) = player
        .single()
        .ok()
        .and_then(|player| player.snake)
        .and_then(|snake| tails.get(snake).ok())
    else {
        return;
    };
    cache.0 = Some(visible_tail(tail.0, tail.1, tail.2));
}

fn death_tail_snapshot(
    message: &PlayerDeath,
    tails: &Query<(&SnakeHead, &TailPoints, Option<&TailLength>)>,
    local_player: Option<(Entity, &Player)>,
    local_tail_cache: &LastLocalSnakeTail,
) -> Option<TailPolyline> {
    let local_tail = || {
        let (player_entity, player) = local_player?;
        if player_entity != message.killed_player {
            return None;
        }
        local_tail_cache.0.clone().or_else(|| {
            player
                .snake
                .and_then(|snake| tails.get(snake).ok())
                .map(|(head, tail, length)| visible_tail(head, tail, length))
        })
    };
    local_tail().or_else(|| {
        tails
            .get(message.killed_snake)
            .ok()
            .map(|(head, tail, length)| visible_tail(head, tail, length))
    })
}

fn death_position(
    message: &PlayerDeath,
    tails: &Query<(&SnakeHead, &TailPoints, Option<&TailLength>)>,
    tail_snapshot: Option<&TailPolyline>,
) -> Option<Vec2> {
    tail_snapshot.map(|tail| tail.front().0).or_else(|| {
        tails
            .get(message.killed_snake)
            .or_else(|_| tails.get(message.killer_snake))
            .ok()
            .map(|(head, _, _)| head.position)
            .or(Some(message.position))
    })
}

fn visible_tail(head: &SnakeHead, tail: &TailPoints, length: Option<&TailLength>) -> TailPolyline {
    tail.polyline(
        head,
        length.map(|length| length.current_size).unwrap_or(0.0),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_death_view_uses_current_life_stats_when_available() {
        let config = GameConfig::default();
        let stats = PlayerStats {
            average_speed: 2.5,
            speed_samples: 12,
            time_alive_seconds: 9.0,
            kills: 3,
            time_as_leader_seconds: 4.0,
            food_eaten: 5,
        };

        let view = fallback_death_view(10.0, &config, Some(42), Some(stats));

        assert_eq!(view.message, "You died");
        assert_eq!(
            view.respawn_allowed_at_seconds,
            10.0 + config.respawn.player_cooldown_seconds
        );
        assert_eq!(
            view.stats,
            Some(PlayerDeathStats {
                average_speed: 2.5,
                score: 42,
                time_alive_seconds: 9.0,
                kills: 3,
                time_as_leader_seconds: 4.0,
                food_eaten: 5,
            })
        );
    }
}
