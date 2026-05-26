use bevy::prelude::*;
use shared::network::protocol::prelude::*;

pub(crate) struct FoodRenderPlugin;

impl FoodRenderPlugin {
    fn draw_food(mut gizmos: Gizmos, query: Query<&Position, With<FoodMarker>>) {
        for pos in query.iter() {
            gizmos.circle_2d(pos.0, 5.0, Color::srgb(0.0, 0.8, 0.2));
        }
    }
}

impl Plugin for FoodRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, FoodRenderPlugin::draw_food);
    }
}
