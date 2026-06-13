use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::{
    ActionMock, ActionValue, EnhancedInputSystems, MockSpan, TriggerState,
};
use lightyear::prelude::input::bei::{Action, ActionOf, InputMarker};
use lightyear::prelude::Controlled;

use crate::network::inputs::AutoRespawnRequests;
use shared::bot::{direction_to_input, BotController};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct BotClientPlugin {
    pub(crate) decision_interval_ticks: u32,
    pub(crate) mistake_chance_per_decision_percent: u8,
}

#[derive(Resource, Clone, Copy, Debug)]
struct BotClientSettings {
    decision_interval_ticks: u32,
    mistake_chance_per_decision_percent: u8,
}

impl Plugin for BotClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BotClientSettings {
            decision_interval_ticks: self.decision_interval_ticks,
            mistake_chance_per_decision_percent: self.mistake_chance_per_decision_percent,
        });
        app.insert_resource(AutoRespawnRequests);
        app.add_systems(Update, (attach_bot_controllers, ensure_move_action_mocks));
        app.add_systems(
            FixedPreUpdate,
            update_move_action_mocks.before(EnhancedInputSystems::Update),
        );
    }
}

fn attach_bot_controllers(
    mut commands: Commands,
    settings: Res<BotClientSettings>,
    snakes: Query<Entity, (With<Controlled>, With<TailPoints>, Without<BotController>)>,
) {
    for snake in &snakes {
        commands
            .entity(snake)
            .insert(BotController::new_with_mistakes(
                settings.decision_interval_ticks,
                snake.to_bits(),
                settings.mistake_chance_per_decision_percent,
            ));
    }
}

fn ensure_move_action_mocks(
    mut commands: Commands,
    actions: Query<
        Entity,
        (
            With<Action<MoveSnake>>,
            With<InputMarker<SnakeInput>>,
            Without<ActionMock>,
        ),
    >,
) {
    for action in &actions {
        commands.entity(action).insert(ActionMock::new(
            TriggerState::Fired,
            Vec2::Y,
            MockSpan::Manual,
        ));
    }
}

fn update_move_action_mocks(
    config: Res<GameConfig>,
    mut snakes: ParamSet<(
        Query<(
            Entity,
            &SnakeHead,
            &TailPoints,
            Option<&TailLength>,
            &RoomId,
        )>,
        Query<
            (
                Entity,
                &SnakeHead,
                &TailPoints,
                Option<&TailLength>,
                &RoomId,
                &mut BotController,
            ),
            With<Controlled>,
        >,
    )>,
    mut actions: Query<
        (&ActionOf<SnakeInput>, &mut ActionMock),
        (With<Action<MoveSnake>>, With<InputMarker<SnakeInput>>),
    >,
) {
    let tail_snapshots = snakes
        .p0()
        .iter()
        .map(|(entity, head, tail, length, room)| (entity, *room, visible_tail(head, tail, length)))
        .collect::<Vec<_>>();

    let mut controlled_snakes = snakes.p1();
    let Ok((snake, head, tail, length, room, mut controller)) = controlled_snakes.single_mut()
    else {
        return;
    };
    let tail = visible_tail(head, tail, length);
    let obstacle_tails = tail_snapshots
        .iter()
        .filter(|(other_entity, other_room, _)| *other_entity != snake && other_room == room)
        .map(|(_, _, tail)| tail)
        .collect::<Vec<_>>();
    let direction = controller.choose_direction_avoiding(&tail, &config.arena, &obstacle_tails);
    let value = ActionValue::Axis2D(direction_to_input(direction));
    for (_, mut mock) in actions
        .iter_mut()
        .filter(|(action_of, _)| action_of.get() == snake)
    {
        mock.state = TriggerState::Fired;
        mock.value = value;
        mock.span = MockSpan::Manual;
        mock.enabled = true;
    }
}

fn visible_tail(head: &SnakeHead, tail: &TailPoints, length: Option<&TailLength>) -> TailPolyline {
    tail.polyline(
        head,
        length.map(|length| length.current_size).unwrap_or(0.0),
    )
}
