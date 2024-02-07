use bevy::prelude::*;
use lightyear::prelude::client::*;
use shared::network::protocol::prelude::*;
use shared::network::protocol::prelude::Direction;

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
    heads: Query<(&HeadPoint), Without<Confirmed>>,
    tails: Query<(&TailParent, &TailPoints), Without<Confirmed>>,
) {
    for (parent, points) in tails.iter() {
        let Ok(position) = heads.get(parent.0) else {
            error!("Tail entity has no parent entity!");
            continue;
        };
        // draw the head
        gizmos.rect_2d(
            position.0,
            0.0,
            Vec2::ONE * 20.0,
            Color::BLUE
        );
        points.pairs(&(position.0, Direction::Up)).for_each(|(start, end)| {
            gizmos.line_2d(start.0, end.0, Color::BLUE);
            if start.0.x != end.0.x && start.0.y != end.0.y {
                info!("DIAGONAL");
            }
        });
    }
}