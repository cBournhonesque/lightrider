use bevy::prelude::*;
use lightyear::frame_interpolation::{FrameInterpolate, FrameInterpolationPlugin};
use lightyear::prelude::*;

use shared::network::protocol::prelude::*;

pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameInterpolationPlugin::<SnakeHead>::default());
        app.add_plugins(FrameInterpolationPlugin::<TailLength>::default());
        app.add_systems(Update, add_frame_interpolation_to_predicted_snakes);
    }
}

fn add_frame_interpolation_to_predicted_snakes(
    mut commands: Commands,
    heads: Query<
        Entity,
        (
            With<Predicted>,
            With<SnakeHead>,
            Without<FrameInterpolate<SnakeHead>>,
        ),
    >,
    lengths: Query<
        Entity,
        (
            With<Predicted>,
            With<TailLength>,
            Without<FrameInterpolate<TailLength>>,
        ),
    >,
) {
    for snake in &heads {
        commands
            .entity(snake)
            .insert(FrameInterpolate::<SnakeHead>::default());
    }
    for snake in &lengths {
        commands
            .entity(snake)
            .insert(FrameInterpolate::<TailLength>::default());
    }
}
