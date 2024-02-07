use std::collections::VecDeque;
use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::{ClientId, NetworkTarget, ReplicationGroup};
use shared::network::protocol::prelude::*;
use shared::network::protocol::prelude::Direction;
use shared::network::protocol::Replicate;


pub const TAIL_SIZE: f32 = 100.0;

#[derive(Bundle)]
pub(crate) struct HeadBundle {
    pub head_point: HeadPoint,
    pub head_direction: HeadDirection,
    pub tail_length: TailLength,
    pub speed: Speed,
    pub acceleration: Acceleration,
    // we need to include the action-state so that client inputs are replicated to the server
    pub action: ActionState<PlayerMovement>,
}

impl Default for HeadBundle {
    fn default() -> Self {
        Self {
            head_point: HeadPoint(Vec2::new(0.0, 0.0)),
            head_direction: HeadDirection(Direction::Up),
            tail_length: TailLength {
                current_size: TAIL_SIZE,
                target_size: TAIL_SIZE,
            },
            speed: Speed(0.5),
            acceleration: Acceleration(0.0),
            action: ActionState::default(),
        }
    }
}

#[derive(Bundle)]
pub struct TailBundle {
    pub tail_points: TailPoints,
    pub tail_parent: TailParent,
}

impl TailBundle {
    pub(crate) fn new(parent_entity: Entity, head_point: Vec2) -> Self {
        let mut tail_points = VecDeque::new();
        tail_points.push_back((head_point + Direction::Down.delta() * TAIL_SIZE, Direction::Up));
        Self {
            tail_points: TailPoints(tail_points),
            tail_parent: TailParent(parent_entity),
        }
    }
}


pub(crate) struct SnakeBundle;

impl SnakeBundle {
    // pub(crate) fn spawn(commands: &mut Commands) {
    //     let mut head_id = commands.spawn(HeadBundle::default());
    //     head_id.with_children(|parent| {
    //         parent.spawn(TailBundle::new(Vec2::default()));
    //     });
    // }

    pub(crate) fn spawn(commands: &mut Commands, client_id: ClientId) {
        let mut replicate = Replicate {
            prediction_target: NetworkTarget::Single(client_id),
            replication_group: Default::default(),
            ..default()
        };
        replicate.add_target::<ActionState<PlayerMovement>>(NetworkTarget::AllExceptSingle(client_id));
        let head_entity = commands.spawn(
            (HeadBundle::default(), replicate)
        ).id();
        let mut replicate = Replicate {
            prediction_target: NetworkTarget::Single(client_id),
            // we want the tail to be part of the same replication group
            replication_group: ReplicationGroup::new_id(head_entity.to_bits()),
            ..default()
        };
        replicate.add_target::<ActionState<PlayerMovement>>(NetworkTarget::AllExceptSingle(client_id));
        commands.spawn((TailBundle::new(head_entity, Vec2::default()), replicate));
    }
}