use bevy::prelude::*;
use lightyear::prelude::{InterpolationTarget, NetworkTarget, PeerId, PredictionTarget, Replicate};

use crate::network::protocol::prelude::Direction;
use crate::network::protocol::prelude::*;

use crate::config::MovementConfig;
use crate::utils::query::SimulationAuthority;

pub const TAIL_SIZE: f32 = 200.0;

#[derive(Bundle)]
pub struct SnakeBundle {
    // main
    pub head: SnakeHead,
    pub tail_length: TailLength,
    pub speed: Speed,
    pub acceleration: Acceleration,
    pub tail_points: TailPoints,
    pub tail_path_history: TailPathHistory,
    pub food_boost: FoodBoost,
    pub input: SnakeInput,
    pub room: RoomId,
}

impl Default for SnakeBundle {
    fn default() -> Self {
        Self::new(&MovementConfig::default())
    }
}

impl SnakeBundle {
    pub fn new(config: &MovementConfig) -> Self {
        Self::new_in_room(config, RoomId::default())
    }

    pub fn new_in_room(config: &MovementConfig, room: RoomId) -> Self {
        Self::new_at(config, room, Vec2::ZERO, Direction::Up)
    }

    pub fn new_at(
        config: &MovementConfig,
        room: RoomId,
        position: Vec2,
        direction: Direction,
    ) -> Self {
        let head = SnakeHead {
            position,
            direction,
        };
        let tail_points = TailPoints::empty();
        Self {
            head,
            tail_points,
            tail_path_history: TailPathHistory::default(),
            tail_length: TailLength {
                current_size: config.starting_tail_length,
                target_size: config.starting_tail_length,
            },
            speed: Speed(config.min_speed),
            acceleration: Acceleration(0.0),
            food_boost: FoodBoost::default(),
            input: SnakeInput,
            room,
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

    pub fn spawn(commands: &mut Commands, client_id: PeerId) -> Entity {
        Self::spawn_bundle(commands, client_id, SnakeBundle::default())
    }

    pub fn spawn_with_movement_config(
        commands: &mut Commands,
        client_id: PeerId,
        config: &MovementConfig,
    ) -> Entity {
        Self::spawn_bundle(commands, client_id, SnakeBundle::new(config))
    }

    pub fn spawn_with_room(
        commands: &mut Commands,
        client_id: PeerId,
        config: &MovementConfig,
        room: RoomId,
    ) -> Entity {
        Self::spawn_bundle(commands, client_id, SnakeBundle::new_in_room(config, room))
    }

    pub fn spawn_with_room_at(
        commands: &mut Commands,
        client_id: PeerId,
        config: &MovementConfig,
        room: RoomId,
        position: Vec2,
        direction: Direction,
    ) -> Entity {
        Self::spawn_bundle(
            commands,
            client_id,
            SnakeBundle::new_at(config, room, position, direction),
        )
    }

    pub fn spawn_server_owned(
        commands: &mut Commands,
        _group_id: u64,
        config: &MovementConfig,
        room: RoomId,
    ) -> Entity {
        commands
            .spawn((
                SnakeBundle::new_in_room(config, room),
                SimulationAuthority,
                Replicate::to_clients(NetworkTarget::All),
                InterpolationTarget::to_clients(NetworkTarget::All),
            ))
            .id()
    }

    pub fn spawn_server_owned_at(
        commands: &mut Commands,
        _group_id: u64,
        config: &MovementConfig,
        room: RoomId,
        position: Vec2,
        direction: Direction,
    ) -> Entity {
        commands
            .spawn((
                SnakeBundle::new_at(config, room, position, direction),
                SimulationAuthority,
                Replicate::to_clients(NetworkTarget::All),
                InterpolationTarget::to_clients(NetworkTarget::All),
            ))
            .id()
    }

    fn spawn_bundle(commands: &mut Commands, client_id: PeerId, bundle: SnakeBundle) -> Entity {
        commands
            .spawn((
                bundle,
                SimulationAuthority,
                Replicate::to_clients(NetworkTarget::All),
                PredictionTarget::to_clients(NetworkTarget::Single(client_id)),
                InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(client_id)),
            ))
            .id()
    }
}
