use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_xpbd_2d::components::Collider;
use bevy_xpbd_2d::parry::shape::SharedShape;
use bevy_xpbd_2d::prelude::{CollisionLayers, Position, Rotation};
use leafwing_input_manager::prelude::ActionState;
use lightyear::prelude::{ClientId, NetworkTarget, ReplicationGroup};

use shared::network::protocol::prelude::*;
use shared::network::protocol::prelude::Direction;
use shared::network::protocol::Replicate;

use crate::collision::layers::CollideLayer;

pub const TAIL_SIZE: f32 = 100.0;

pub const START_SPEED: f32 = 0.5;

#[derive(Bundle)]
pub(crate) struct SnakeBundle {
    // main
    pub tail_length: TailLength,
    pub speed: Speed,
    pub acceleration: Acceleration,
    pub tail_points: TailPoints,
    // physics
    // NOTE: position/rotation are necessary for spatial queries (to compute an isometry). Otherwise we don't really use them
    //  so let's leave them at default
    pub position: Position,
    pub rotation: Rotation,
    pub collider: Collider,
    pub collider_layers: CollisionLayers,
    // we need to include the action-state so that client inputs are replicated to the server
    pub action: ActionState<PlayerMovement>,
}

impl Default for SnakeBundle {
    fn default() -> Self {
        let tail_points = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, 0.0) + Direction::Down.delta() * TAIL_SIZE, Direction::Up),
        ]));
        let collider = Collider::from(SharedShape::polyline(tail_points.points_front_to_back(), None));
        Self {
            tail_points,
            tail_length: TailLength {
                current_size: TAIL_SIZE,
                target_size: TAIL_SIZE,
            },
            speed: Speed(START_SPEED),
            acceleration: Acceleration(0.0),
            position: Position::default(),
            rotation: Rotation::default(),
            collider,
            collider_layers: CollisionLayers::new([CollideLayer::Player], [CollideLayer::Player, CollideLayer::Wall, CollideLayer::Food]),
            action: ActionState::default(),
        }
    }
}

impl SnakeBundle {
    // pub(crate) fn spawn(commands: &mut Commands) {
    //     let mut head_id = commands.spawn(HeadBundle::default());
    //     head_id.with_children(|parent| {
    //         parent.spawn(TailBundle::new(Vec2::default()));
    //     });
    // }

    pub(crate) fn spawn(commands: &mut Commands, client_id: ClientId) -> Entity {
        let mut replicate = Replicate {
            prediction_target: NetworkTarget::Single(client_id),
            interpolation_target: NetworkTarget::AllExceptSingle(client_id),
            replication_group: ReplicationGroup::new_id(client_id),
            ..default()
        };
        // we do not need to replicate the player's actions
        replicate.disable_component::<ActionState<PlayerMovement>>();
        let head_entity = commands.spawn(
            (
                SnakeBundle::default(),
                replicate,
            )
        ).id();
        head_entity
    }
}