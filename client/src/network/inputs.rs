use bevy::app::{App, FixedPreUpdate, Plugin};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use lightyear::prelude::input::bei::{
    Action, ActionMock, ActionOf, Actions, Bindings, Cardinal, EnhancedInputSystems, InputMarker,
    TriggerState,
};
#[cfg(target_family = "wasm")]
use lightyear::prelude::Link;
use lightyear::prelude::{
    Client, ConfirmHistory, ConfirmedHistory, Connected, Controlled, ControlledBy, LocalId,
    MessageSender, PeerId, Predicted,
};

use crate::collision::death::DeathView;
use shared::network::protocol::prelude::*;

pub struct NetworkInputsPlugin;

#[derive(Resource)]
pub(crate) struct AutoRespawnRequests;

#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct TurnStressSettings {
    pub(crate) hz: f32,
    pub(crate) duration_seconds: Option<f32>,
}

#[cfg(target_family = "wasm")]
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct BrowserRttProbe;

#[derive(Default)]
struct TurnStressState {
    next_turn_at_seconds: f64,
    started_at_seconds: Option<f64>,
    sequence: u64,
    logged_waiting: bool,
}

impl Plugin for NetworkInputsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (send_player_spawn_requests, ensure_snake_inputs));
        #[cfg(target_family = "wasm")]
        app.add_systems(Update, record_browser_rtt_probe);
        app.add_systems(
            FixedPreUpdate,
            drive_turn_stress.before(EnhancedInputSystems::Update),
        );
        app.add_observer(bind_snake_inputs_when_context_is_controlled);
        app.add_observer(bind_snake_inputs_when_actions_are_added);
        app.add_observer(bind_snake_inputs_when_action_is_added);
        app.add_observer(bind_snake_inputs_when_action_component_is_added);
        app.add_observer(bind_snake_inputs_when_action_is_confirmed);
    }
}

fn drive_turn_stress(
    settings: Option<Res<TurnStressSettings>>,
    time: Res<Time<Real>>,
    mut state: Local<TurnStressState>,
    mut commands: Commands,
    actions: Query<
        (Entity, &ActionOf<SnakeInput>),
        (With<Action<MoveSnake>>, With<InputMarker<SnakeInput>>),
    >,
    heads: Query<&SnakeHead>,
) {
    let Some(settings) = settings else {
        return;
    };
    let hz = settings.hz.max(0.0);
    if hz <= 0.0 {
        return;
    }

    let now = time.elapsed_secs_f64();
    let started_at = *state.started_at_seconds.get_or_insert(now);
    if settings
        .duration_seconds
        .is_some_and(|duration| now - started_at >= f64::from(duration.max(0.0)))
    {
        return;
    }
    if now < state.next_turn_at_seconds {
        return;
    }

    let Some((action, action_of, head)) = actions.iter().find_map(|(action, action_of)| {
        heads
            .get(action_of.get())
            .ok()
            .map(|head| (action, action_of, head))
    }) else {
        if !state.logged_waiting {
            info!("Turn stress waiting for a locally controlled snake input action");
            state.logged_waiting = true;
        }
        state.next_turn_at_seconds = now + 0.1;
        return;
    };

    let direction = turn_stress_direction(head.direction, state.sequence);
    commands
        .entity(action)
        .insert(ActionMock::once(TriggerState::Fired, direction));
    tracing::trace!(
        target: "lightyear_debug::manual",
        kind = "turn_stress_input",
        sample_point = "FixedPreUpdate",
        schedule = "FixedPreUpdate",
        action = ?action,
        context = ?action_of.get(),
        sequence = state.sequence,
        direction_x = direction.x,
        direction_y = direction.y,
        "generated headless turn-stress input"
    );
    #[cfg(target_family = "wasm")]
    push_browser_turn_stress_input(now, state.sequence, direction);
    state.sequence += 1;
    state.next_turn_at_seconds = now + 1.0 / f64::from(hz);
}

#[cfg(target_family = "wasm")]
fn record_browser_rtt_probe(
    probe: Option<Res<BrowserRttProbe>>,
    time: Res<Time<Real>>,
    links: Query<&Link, (With<Client>, With<Connected>)>,
    mut next_sample_at_seconds: Local<f64>,
) {
    if probe.is_none() {
        return;
    }
    let now = time.elapsed_secs_f64();
    if now < *next_sample_at_seconds {
        return;
    }
    *next_sample_at_seconds = now + 0.25;

    let Ok(link) = links.single() else {
        return;
    };
    push_browser_rtt_sample(
        now,
        link.stats.rtt.as_secs_f64() * 1000.0,
        link.stats.jitter.as_secs_f64() * 1000.0,
    );
}

#[cfg(target_family = "wasm")]
fn push_browser_rtt_sample(time_seconds: f64, rtt_ms: f64, jitter_ms: f64) {
    let Some(samples) = browser_sample_array("__lightriderRttSamples") else {
        return;
    };
    let sample = js_sys::Object::new();
    set_browser_property(sample.as_ref(), "timeSeconds", time_seconds);
    set_browser_property(sample.as_ref(), "rttMs", rtt_ms);
    set_browser_property(sample.as_ref(), "jitterMs", jitter_ms);
    samples.push(sample.as_ref());
}

#[cfg(target_family = "wasm")]
fn push_browser_turn_stress_input(time_seconds: f64, sequence: u64, direction: Vec2) {
    let Some(samples) = browser_sample_array("__lightriderTurnStressInputs") else {
        return;
    };
    let sample = js_sys::Object::new();
    set_browser_property(sample.as_ref(), "timeSeconds", time_seconds);
    set_browser_property(sample.as_ref(), "sequence", sequence as f64);
    set_browser_property(sample.as_ref(), "directionX", f64::from(direction.x));
    set_browser_property(sample.as_ref(), "directionY", f64::from(direction.y));
    samples.push(sample.as_ref());
}

#[cfg(target_family = "wasm")]
fn browser_sample_array(name: &str) -> Option<js_sys::Array> {
    let window = web_sys::window()?;
    let key = wasm_bindgen::JsValue::from_str(name);
    if let Ok(value) = js_sys::Reflect::get(window.as_ref(), &key) {
        if js_sys::Array::is_array(&value) {
            return Some(value.into());
        }
    }

    let samples = js_sys::Array::new();
    let _ = js_sys::Reflect::set(window.as_ref(), &key, samples.as_ref());
    Some(samples)
}

#[cfg(target_family = "wasm")]
fn set_browser_property(
    object: &wasm_bindgen::JsValue,
    key: &str,
    value: impl Into<wasm_bindgen::JsValue>,
) {
    let _ = js_sys::Reflect::set(object, &wasm_bindgen::JsValue::from_str(key), &value.into());
}

fn turn_stress_direction(_current: Direction, sequence: u64) -> Vec2 {
    if sequence.is_multiple_of(2) {
        -Vec2::X
    } else {
        Vec2::Y
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
    client: Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: Query<(&Player, Has<Controlled>)>,
    snake_owners: Query<
        (Option<&HasPlayer>, Option<&ConfirmedHistory<HasPlayer>>),
        (With<TailPoints>, With<SnakeInput>),
    >,
) {
    let Ok((client_entity, local_id)) = client.single() else {
        return;
    };
    let local_id = local_id.map(|local_id| local_id.0);
    for (context, is_controlled, controlled_by, move_actions) in &snake_contexts {
        bind_snake_inputs(
            &mut commands,
            context,
            client_entity,
            local_id,
            is_controlled,
            controlled_by,
            move_actions,
            &action_entities,
            &players,
        );
    }
    for (action, action_of, has_input_marker, has_bindings, has_confirm_history) in &action_entities
    {
        bind_snake_input_action_for_context(
            &mut commands,
            action,
            action_of.get(),
            client_entity,
            local_id,
            has_input_marker,
            has_bindings,
            has_confirm_history,
            &snake_contexts,
            &players,
        );
    }
    for (action, action_of, has_input_marker, has_bindings, has_confirm_history) in &action_entities
    {
        let context = action_of.get();
        if !snake_is_owned_by_local_player(context, &snake_owners, &players, local_id) {
            continue;
        }
        commands
            .entity(context)
            .insert(InputMarker::<SnakeInput>::default());
        bind_snake_input_action(
            &mut commands,
            action,
            has_input_marker,
            has_bindings,
            has_confirm_history,
        );
    }
}

fn bind_snake_inputs_when_context_is_controlled(
    trigger: On<Add, Controlled>,
    mut commands: Commands,
    client: Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: Query<(&Player, Has<Controlled>)>,
) {
    bind_snake_inputs_for_context(
        trigger.entity,
        &mut commands,
        &client,
        &snake_contexts,
        &action_entities,
        &players,
    );
}

fn bind_snake_inputs_when_actions_are_added(
    trigger: On<Add, Actions<SnakeInput>>,
    mut commands: Commands,
    client: Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: Query<(&Player, Has<Controlled>)>,
) {
    bind_snake_inputs_for_context(
        trigger.entity,
        &mut commands,
        &client,
        &snake_contexts,
        &action_entities,
        &players,
    );
}

fn bind_snake_inputs_when_action_is_added(
    trigger: On<Add, ActionOf<SnakeInput>>,
    action_of: Query<&ActionOf<SnakeInput>, With<Action<MoveSnake>>>,
    mut commands: Commands,
    client: Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: Query<(&Player, Has<Controlled>)>,
) {
    let Ok(action_of) = action_of.get(trigger.entity) else {
        return;
    };
    bind_snake_inputs_for_action(
        trigger.entity,
        action_of.get(),
        &mut commands,
        &client,
        &snake_contexts,
        &action_entities,
        &players,
    );
}

fn bind_snake_inputs_when_action_component_is_added(
    trigger: On<Add, Action<MoveSnake>>,
    action_of: Query<&ActionOf<SnakeInput>, With<Action<MoveSnake>>>,
    mut commands: Commands,
    client: Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: Query<(&Player, Has<Controlled>)>,
) {
    let Ok(action_of) = action_of.get(trigger.entity) else {
        return;
    };
    bind_snake_inputs_for_action(
        trigger.entity,
        action_of.get(),
        &mut commands,
        &client,
        &snake_contexts,
        &action_entities,
        &players,
    );
}

fn bind_snake_inputs_when_action_is_confirmed(
    trigger: On<Add, ConfirmHistory>,
    action_of: Query<&ActionOf<SnakeInput>, With<Action<MoveSnake>>>,
    mut commands: Commands,
    client: Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: Query<(&Player, Has<Controlled>)>,
) {
    let Ok(action_of) = action_of.get(trigger.entity) else {
        return;
    };
    bind_snake_inputs_for_action(
        trigger.entity,
        action_of.get(),
        &mut commands,
        &client,
        &snake_contexts,
        &action_entities,
        &players,
    );
}

fn bind_snake_inputs_for_context(
    context: Entity,
    commands: &mut Commands,
    client: &Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: &Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: &Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: &Query<(&Player, Has<Controlled>)>,
) {
    let Ok((client_entity, local_id)) = client.single() else {
        return;
    };
    let local_id = local_id.map(|local_id| local_id.0);
    let Ok((context, is_controlled, controlled_by, move_actions)) = snake_contexts.get(context)
    else {
        return;
    };
    bind_snake_inputs(
        commands,
        context,
        client_entity,
        local_id,
        is_controlled,
        controlled_by,
        move_actions,
        action_entities,
        players,
    );
}

fn bind_snake_inputs_for_action(
    action: Entity,
    context: Entity,
    commands: &mut Commands,
    client: &Query<(Entity, Option<&LocalId>), With<Client>>,
    snake_contexts: &Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    action_entities: &Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: &Query<(&Player, Has<Controlled>)>,
) {
    let Ok((client_entity, local_id)) = client.single() else {
        return;
    };
    let local_id = local_id.map(|local_id| local_id.0);
    let Ok((action, _, has_input_marker, has_bindings, has_confirm_history)) =
        action_entities.get(action)
    else {
        return;
    };
    bind_snake_input_action_for_context(
        commands,
        action,
        context,
        client_entity,
        local_id,
        has_input_marker,
        has_bindings,
        has_confirm_history,
        snake_contexts,
        players,
    );
}

fn bind_snake_inputs(
    commands: &mut Commands,
    context: Entity,
    client_entity: Entity,
    local_id: Option<PeerId>,
    is_controlled: bool,
    controlled_by: Option<&ControlledBy>,
    move_actions: Option<&Actions<SnakeInput>>,
    action_entities: &Query<
        (
            Entity,
            &ActionOf<SnakeInput>,
            Has<InputMarker<SnakeInput>>,
            Has<Bindings>,
            Has<ConfirmHistory>,
        ),
        With<Action<MoveSnake>>,
    >,
    players: &Query<(&Player, Has<Controlled>)>,
) {
    if !snake_context_targets_local_player(
        context,
        client_entity,
        local_id,
        is_controlled,
        controlled_by,
        players,
    ) {
        return;
    }

    commands
        .entity(context)
        .insert(InputMarker::<SnakeInput>::default());

    let Some(move_actions) = move_actions else {
        return;
    };

    for action in move_actions.iter() {
        let Ok((action, _, has_input_marker, has_bindings, has_confirm_history)) =
            action_entities.get(action)
        else {
            continue;
        };
        bind_snake_input_action(
            commands,
            action,
            has_input_marker,
            has_bindings,
            has_confirm_history,
        );
    }
}

fn bind_snake_input_action_for_context(
    commands: &mut Commands,
    action: Entity,
    context: Entity,
    client_entity: Entity,
    local_id: Option<PeerId>,
    has_input_marker: bool,
    has_bindings: bool,
    has_confirm_history: bool,
    snake_contexts: &Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    players: &Query<(&Player, Has<Controlled>)>,
) {
    if !action_targets_local_snake(context, client_entity, local_id, snake_contexts, players) {
        return;
    }
    bind_snake_input_action(
        commands,
        action,
        has_input_marker,
        has_bindings,
        has_confirm_history,
    );
}

fn bind_snake_input_action(
    commands: &mut Commands,
    action: Entity,
    has_input_marker: bool,
    has_bindings: bool,
    _has_confirm_history: bool,
) {
    let mut action_commands = commands.entity(action);
    if !has_bindings {
        action_commands.insert(Bindings::spawn((Cardinal::wasd_keys(), Cardinal::arrows())));
        info!(?action, "Bound replicated snake input action");
    }
    if !has_input_marker {
        action_commands.insert(InputMarker::<SnakeInput>::default());
        info!(
            ?action,
            "Marked replicated snake input action for local input"
        );
    }
}

fn action_targets_local_snake(
    context: Entity,
    client_entity: Entity,
    local_id: Option<PeerId>,
    snake_contexts: &Query<
        (
            Entity,
            Has<Controlled>,
            Option<&ControlledBy>,
            Option<&Actions<SnakeInput>>,
        ),
        (With<TailPoints>, With<SnakeInput>),
    >,
    players: &Query<(&Player, Has<Controlled>)>,
) -> bool {
    if players.iter().any(|(player, is_controlled)| {
        local_player_targets_snake(player, is_controlled, local_id, context)
    }) {
        return true;
    }

    snake_contexts
        .get(context)
        .is_ok_and(|(_, is_controlled, controlled_by, _)| {
            snake_context_targets_local_player(
                context,
                client_entity,
                local_id,
                is_controlled,
                controlled_by,
                players,
            )
        })
}

fn snake_context_targets_local_player(
    context: Entity,
    client_entity: Entity,
    local_id: Option<PeerId>,
    is_controlled: bool,
    controlled_by: Option<&ControlledBy>,
    players: &Query<(&Player, Has<Controlled>)>,
) -> bool {
    (is_controlled
        && controlled_by.is_none_or(|controlled_by| controlled_by.owner == client_entity))
        || players.iter().any(|(player, is_controlled)| {
            local_player_targets_snake(player, is_controlled, local_id, context)
        })
}

fn snake_is_owned_by_local_player(
    context: Entity,
    snake_owners: &Query<
        (Option<&HasPlayer>, Option<&ConfirmedHistory<HasPlayer>>),
        (With<TailPoints>, With<SnakeInput>),
    >,
    players: &Query<(&Player, Has<Controlled>)>,
    local_id: Option<PeerId>,
) -> bool {
    let Ok((owner, owner_history)) = snake_owners.get(context) else {
        return false;
    };
    let Some(player_entity) = snake_owner(owner, owner_history) else {
        return false;
    };
    players
        .get(player_entity)
        .is_ok_and(|(player, is_controlled)| local_player_matches(player, is_controlled, local_id))
}

fn local_player_targets_snake(
    player: &Player,
    is_controlled: bool,
    local_id: Option<PeerId>,
    context: Entity,
) -> bool {
    player.snake == Some(context) && local_player_matches(player, is_controlled, local_id)
}

fn local_player_matches(player: &Player, is_controlled: bool, local_id: Option<PeerId>) -> bool {
    is_controlled || local_id.is_some_and(|local_id| player.id == local_id)
}

fn snake_owner(
    owner: Option<&HasPlayer>,
    owner_history: Option<&ConfirmedHistory<HasPlayer>>,
) -> Option<Entity> {
    owner.map(|owner| owner.0).or_else(|| {
        owner_history.and_then(|history| history.newest_present().map(|(_, owner)| owner.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_replicon::prelude::RepliconTick;

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

    #[test]
    fn ensure_snake_inputs_binds_replicated_server_action() {
        let mut app = App::new();
        app.add_systems(Update, ensure_snake_inputs);

        let client = app.world_mut().spawn(Client::default()).id();
        let snake = app
            .world_mut()
            .spawn((
                TailPoints::empty(),
                SnakeInput,
                Predicted,
                ControlledBy {
                    owner: client,
                    lifetime: Default::default(),
                },
            ))
            .id();
        let action = app
            .world_mut()
            .spawn((
                ActionOf::<SnakeInput>::new(snake),
                Action::<MoveSnake>::new(),
                ConfirmHistory::new(RepliconTick::new(1)),
            ))
            .id();

        app.update();

        let mut actions = app
            .world_mut()
            .query_filtered::<Entity, With<Action<MoveSnake>>>();
        assert_eq!(actions.iter(app.world()).count(), 1);
        let action_ref = app.world().entity(action);
        assert!(action_ref.contains::<Bindings>());
        assert!(action_ref.contains::<InputMarker<SnakeInput>>());
        assert!(app
            .world()
            .entity(snake)
            .contains::<InputMarker<SnakeInput>>());
        assert!(app
            .world()
            .entity(snake)
            .get::<Actions<SnakeInput>>()
            .is_some_and(|actions| actions.iter().any(|entity| entity == action)));
    }

    #[test]
    fn ensure_snake_inputs_marks_bound_action_before_confirm_history_arrives() {
        let mut app = App::new();
        app.add_systems(Update, ensure_snake_inputs);

        let client = app.world_mut().spawn(Client::default()).id();
        let snake = app
            .world_mut()
            .spawn((
                TailPoints::empty(),
                SnakeInput,
                Predicted,
                ControlledBy {
                    owner: client,
                    lifetime: Default::default(),
                },
            ))
            .id();
        let action = app
            .world_mut()
            .spawn((
                ActionOf::<SnakeInput>::new(snake),
                Action::<MoveSnake>::new(),
            ))
            .id();

        app.update();
        let action_ref = app.world().entity(action);
        assert!(action_ref.contains::<Bindings>());
        assert!(action_ref.contains::<InputMarker<SnakeInput>>());
        assert!(app
            .world()
            .entity(snake)
            .contains::<InputMarker<SnakeInput>>());

        app.world_mut()
            .entity_mut(action)
            .insert(ConfirmHistory::new(RepliconTick::new(1)));
        app.update();

        let action_ref = app.world().entity(action);
        assert!(action_ref.contains::<InputMarker<SnakeInput>>());
    }

    #[test]
    fn ensure_snake_inputs_binds_first_predicted_snake_from_controlled_player() {
        let mut app = App::new();
        app.add_systems(Update, ensure_snake_inputs);

        app.world_mut().spawn(Client::default());
        let snake = app
            .world_mut()
            .spawn((TailPoints::empty(), SnakeInput, Predicted))
            .id();
        let action = app
            .world_mut()
            .spawn((
                ActionOf::<SnakeInput>::new(snake),
                Action::<MoveSnake>::new(),
                ConfirmHistory::new(RepliconTick::new(1)),
            ))
            .id();
        app.world_mut().spawn((
            Player {
                id: lightyear::prelude::PeerId::Netcode(1),
                name: "local".to_string(),
                snake: Some(snake),
            },
            PlayerStatus::Alive,
            Controlled,
        ));

        app.update();

        let action_ref = app.world().entity(action);
        assert!(action_ref.contains::<Bindings>());
        assert!(action_ref.contains::<InputMarker<SnakeInput>>());
    }

    #[test]
    fn ensure_snake_inputs_binds_local_id_player_without_controlled_marker() {
        let mut app = App::new();
        app.add_systems(Update, ensure_snake_inputs);

        let local_id = PeerId::Netcode(1);
        app.world_mut()
            .spawn((Client::default(), LocalId(local_id)));
        let snake = app
            .world_mut()
            .spawn((TailPoints::empty(), SnakeInput))
            .id();
        let action = app
            .world_mut()
            .spawn((
                ActionOf::<SnakeInput>::new(snake),
                Action::<MoveSnake>::new(),
                ConfirmHistory::new(RepliconTick::new(1)),
            ))
            .id();
        app.world_mut().spawn((
            Player {
                id: local_id,
                name: "local".to_string(),
                snake: Some(snake),
            },
            PlayerStatus::Alive,
        ));

        app.update();

        let action_ref = app.world().entity(action);
        assert!(action_ref.contains::<Bindings>());
        assert!(action_ref.contains::<InputMarker<SnakeInput>>());
        assert!(app
            .world()
            .entity(snake)
            .contains::<InputMarker<SnakeInput>>());
    }

    #[test]
    fn ensure_snake_inputs_binds_local_owned_snake_without_player_snake_reference() {
        let mut app = App::new();
        app.add_systems(Update, ensure_snake_inputs);

        let local_id = PeerId::Netcode(1);
        app.world_mut()
            .spawn((Client::default(), LocalId(local_id)));
        let player = app
            .world_mut()
            .spawn((
                Player {
                    id: local_id,
                    name: "local".to_string(),
                    snake: None,
                },
                PlayerStatus::Alive,
            ))
            .id();
        let snake = app
            .world_mut()
            .spawn((TailPoints::empty(), SnakeInput, HasPlayer(player)))
            .id();
        let action = app
            .world_mut()
            .spawn((
                ActionOf::<SnakeInput>::new(snake),
                Action::<MoveSnake>::new(),
                ConfirmHistory::new(RepliconTick::new(1)),
            ))
            .id();

        app.update();

        let action_ref = app.world().entity(action);
        assert!(action_ref.contains::<Bindings>());
        assert!(action_ref.contains::<InputMarker<SnakeInput>>());
        assert!(app
            .world()
            .entity(snake)
            .contains::<InputMarker<SnakeInput>>());
    }

    #[test]
    fn ensure_snake_inputs_binds_local_player_action_before_prediction_marker() {
        let mut app = App::new();
        app.add_systems(Update, ensure_snake_inputs);

        app.world_mut().spawn(Client::default());
        let snake = app
            .world_mut()
            .spawn((TailPoints::empty(), SnakeInput))
            .id();
        let action = app
            .world_mut()
            .spawn((
                ActionOf::<SnakeInput>::new(snake),
                Action::<MoveSnake>::new(),
                ConfirmHistory::new(RepliconTick::new(1)),
            ))
            .id();
        app.world_mut().spawn((
            Player {
                id: lightyear::prelude::PeerId::Netcode(1),
                name: "local".to_string(),
                snake: Some(snake),
            },
            PlayerStatus::Alive,
            Controlled,
        ));

        app.update();

        let action_ref = app.world().entity(action);
        assert!(action_ref.contains::<Bindings>());
        assert!(action_ref.contains::<InputMarker<SnakeInput>>());
    }
}
