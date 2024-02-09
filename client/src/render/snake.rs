use bevy::prelude::*;
use lightyear::prelude::client::*;

use shared::network::protocol::prelude::*;

pub(crate) struct SnakeRenderPlugin;

impl Plugin for SnakeRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, draw_snakes);
    }
}

/// System that draws the boxed of the player positions.
/// The components should be replicated from the server to the client
pub(crate) fn draw_snakes(
    mut gizmos: Gizmos,
    tails: Query<&TailPoints, Without<Confirmed>>,
) {
    for points in tails.iter() {
        // draw the head
        gizmos.rect_2d(
            points.front().0,
            0.0,
            Vec2::ONE * 10.0,
            Color::BLUE
        );
        points.pairs_front_to_back().for_each(|(start, end)| {
            gizmos.line_2d(start.0, end.0, Color::BLUE);
            if start.0.x != end.0.x && start.0.y != end.0.y {
                info!("DIAGONAL");
            }
        });
    }
}