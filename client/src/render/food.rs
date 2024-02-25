use bevy::prelude::*;
use shared::network::protocol::prelude::*;

pub(crate) struct FoodRenderPlugin;


impl FoodRenderPlugin {
    fn draw_food(
        mut gizmos: Gizmos,
        query: Query<&Position, With<FoodMarker>>,
    ) {
        for pos in query.iter() {
            gizmos.circle_2d(pos.0, 5.0, Color::GREEN);
        }
    }
}


impl Plugin for FoodRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, FoodRenderPlugin::draw_food);
    }
}