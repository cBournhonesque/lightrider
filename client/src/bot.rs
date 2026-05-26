use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::{
    ActionMock, ActionValue, EnhancedInputSystems, MockSpan, TriggerState,
};
use lightyear::prelude::input::bei::{Action, ActionOf, InputMarker};
use lightyear::prelude::Controlled;

use shared::bot::{direction_to_input, BotController};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct BotClientPlugin {
    pub(crate) decision_interval_ticks: u32,
}

#[derive(Resource, Clone, Copy, Debug)]
struct BotClientSettings {
    decision_interval_ticks: u32,
}

impl Plugin for BotClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BotClientSettings {
            decision_interval_ticks: self.decision_interval_ticks,
        });
        app.add_systems(
            Update,
            (
                attach_bot_controllers,
                ensure_move_action_mocks,
                ensure_spawn_action_mocks,
            ),
        );
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
        commands.entity(snake).insert(BotController::new(
            settings.decision_interval_ticks,
            snake.to_bits(),
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

fn ensure_spawn_action_mocks(
    mut commands: Commands,
    actions: Query<
        Entity,
        (
            With<Action<SpawnPlayer>>,
            With<InputMarker<PlayerInput>>,
            Without<ActionMock>,
        ),
    >,
) {
    for action in &actions {
        commands.entity(action).insert(ActionMock::new(
            TriggerState::Fired,
            true,
            MockSpan::Manual,
        ));
    }
}

fn update_move_action_mocks(
    config: Res<GameConfig>,
    mut snakes: ParamSet<(
        Query<(Entity, &TailPoints, &RoomId)>,
        Query<(Entity, &TailPoints, &RoomId, &mut BotController), With<Controlled>>,
    )>,
    mut actions: Query<
        (&ActionOf<SnakeInput>, &mut ActionMock),
        (With<Action<MoveSnake>>, With<InputMarker<SnakeInput>>),
    >,
) {
    let tail_snapshots = snakes
        .p0()
        .iter()
        .map(|(entity, tail, room)| (entity, *room, tail.clone()))
        .collect::<Vec<_>>();

    let mut controlled_snakes = snakes.p1();
    let Ok((snake, tail, room, mut controller)) = controlled_snakes.single_mut() else {
        return;
    };
    let obstacle_tails = tail_snapshots
        .iter()
        .filter(|(other_entity, other_room, _)| *other_entity != snake && other_room == room)
        .map(|(_, _, tail)| tail)
        .collect::<Vec<_>>();
    let direction = controller.choose_direction_avoiding(tail, &config.arena, &obstacle_tails);
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
