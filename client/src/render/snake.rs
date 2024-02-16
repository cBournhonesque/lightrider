use bevy::prelude::*;
use bevy::transform::TransformSystem;
use lightyear::prelude::client::*;
use lightyear::prelude::{TickManager};
use shared::network::protocol::GameProtocol;

use shared::network::protocol::prelude::*;

pub(crate) struct SnakeRenderPlugin;

impl Plugin for SnakeRenderPlugin {
    fn build(&self, app: &mut App) {
        // Plugins
        // Visually interpolate the tails since they are updated during FixedUpdate
        app.add_plugins(VisualInterpolationPlugin::<TailPoints, GameProtocol>::default());
        // Draw the snakes after visual interpolation is computed
        app.add_systems(PostUpdate, draw_snakes
            .before(TransformSystem::TransformPropagate)
            .after(InterpolationSet::VisualInterpolation)
        );
        // Add visual interpolation after the component gets added on the predicted entity
        app.add_systems(PreUpdate, add_visual_interpolation_to_predicted_snake.after(
            PredictionSet::SpawnHistoryFlush
        ));
    }
}

/// Adds visual interpolation to the predicted tails
fn add_visual_interpolation_to_predicted_snake(
    mut commands: Commands,
    query: Query<Entity, (With<Predicted>, Added<TailPoints>)>
) {
    for entity in query.iter() {
        commands.entity(entity).insert(VisualInterpolateStatus::<TailPoints>::default());
    }
}

/// System that draws the boxed of the player positions.
/// The components should be replicated from the server to the client
pub(crate) fn draw_snakes(
    mut gizmos: Gizmos,
    tails: Query<&TailPoints, Without<Confirmed>>,
    interp_snake: Query<&TailPoints, With<Interpolated>>,
    predicted_snake: Query<&TailPoints, With<Predicted>>,
    tick: Res<TickManager>,
) {
    let tick = tick.tick();
    for points in interp_snake.iter() {
        info!(?tick, front = ?points.front(), "interp snake");
    }
    for points in predicted_snake.iter() {
        info!(?tick, front = ?points.front(), "predicted snake");
    }
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