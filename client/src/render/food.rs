use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct FoodRenderPlugin;

#[derive(Resource, Default)]
struct FoodVisualPositions(EntityHashMap<Vec2>);

impl FoodRenderPlugin {
    fn draw_food(
        mut gizmos: Gizmos,
        config: Res<GameConfig>,
        mut visual_positions: ResMut<FoodVisualPositions>,
        query: Query<(Entity, &Position), With<FoodMarker>>,
    ) {
        let radius = config.food.visual_radius.max(1.0);
        let lerp = config.render.food_visual_lerp.clamp(0.0, 1.0);
        visual_positions
            .0
            .retain(|entity, _| query.contains(*entity));
        for (entity, pos) in query.iter() {
            let visual = visual_positions.0.entry(entity).or_insert(pos.0);
            *visual = visual.lerp(pos.0, lerp);
            gizmos.circle_2d(*visual, radius, Color::srgb(0.25, 1.0, 0.38));
            gizmos.circle_2d(*visual, radius * 0.45, Color::srgb(0.75, 1.0, 0.78));
        }
    }
}

impl Plugin for FoodRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FoodVisualPositions>();
        app.add_systems(Update, FoodRenderPlugin::draw_food);
    }
}
