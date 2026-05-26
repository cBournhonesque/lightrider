use bevy::ecs::query::Or;
use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Interpolated, Predicted, Replicated};

use shared::network::protocol::prelude::*;

pub(crate) struct SnakeRenderPlugin;

impl Plugin for SnakeRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            draw_snakes.after(FrameInterpolationSystems::Interpolate),
        );
    }
}

/// Draw predicted/local snakes, interpolated remote snakes, and server-owned snakes.
pub(crate) fn draw_snakes(
    mut gizmos: Gizmos,
    tails: Query<&TailPoints, Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>>,
) {
    let color = Color::srgb(0.0, 0.25, 1.0);
    for points in tails.iter() {
        gizmos.rect_2d(points.front().0, Vec2::ONE * 10.0, color);
        points.pairs_front_to_back().for_each(|(start, end)| {
            gizmos.line_2d(start.0, end.0, color);
            if start.0.x != end.0.x && start.0.y != end.0.y {
                info!("DIAGONAL");
            }
        });
    }
}
