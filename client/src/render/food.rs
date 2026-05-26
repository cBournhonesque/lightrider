use bevy::prelude::*;
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct FoodRenderPlugin;

impl FoodRenderPlugin {
    fn draw_food(
        mut gizmos: Gizmos,
        config: Res<GameConfig>,
        query: Query<&Position, With<FoodMarker>>,
    ) {
        let radius = config.food.radius.max(8.0);
        for pos in query.iter() {
            gizmos.circle_2d(pos.0, radius, Color::srgb(0.25, 1.0, 0.38));
            gizmos.circle_2d(pos.0, radius * 0.45, Color::srgb(0.75, 1.0, 0.78));
        }
    }
}

impl Plugin for FoodRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, FoodRenderPlugin::draw_food);
    }
}
