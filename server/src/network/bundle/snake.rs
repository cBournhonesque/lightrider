use std::collections::VecDeque;
use bevy::prelude::*;
use shared::network::protocol::prelude::*;
use shared::network::protocol::prelude::Direction;


pub const TAIL_SIZE: f32 = 100.0;

#[derive(Bundle)]
pub(crate) struct HeadBundle {
    pub head_point: HeadPoint,
    pub head_direction: HeadDirection,
    pub tail_length: TailLength,
    pub speed: Speed,
    pub acceleration: Acceleration,
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
            speed: Speed(10.0),
            acceleration: Acceleration(0.0),
        }
    }
}

#[derive(Bundle)]
pub struct TailBundle {
    pub tail_points: TailPoints,
}

impl TailBundle {
    pub(crate) fn new(head_point: Vec2) -> Self {
        let mut tail_points = VecDeque::new();
        tail_points.push_back((head_point + Direction::Down.delta() * TAIL_SIZE, Direction::Up));
        Self {
            tail_points: TailPoints(tail_points),
        }
    }
}


pub(crate) struct SnakeBundle;

impl SnakeBundle {
    pub(crate) fn spawn(commands: &mut Commands) {
        let mut head_id = commands.spawn(HeadBundle::default());
        head_id.with_children(|parent| {
            parent.spawn(TailBundle::new(Vec2::default()));
        });
    }
}