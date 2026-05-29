use bevy::prelude::*;
use std::collections::HashSet;

use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct FoodRenderPlugin;

const FOOD_Z: f32 = 2.0;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FoodVisual {
    target: Entity,
}

impl FoodRenderPlugin {
    fn draw_food(
        mut gizmos: Gizmos,
        config: Res<GameConfig>,
        query: Query<&Position, With<FoodMarker>>,
    ) {
        if config.render.use_assets {
            return;
        }

        let radius = config.food.visual_radius.max(1.0);
        for pos in query.iter() {
            gizmos.circle_2d(pos.0, radius, Color::srgb(0.25, 1.0, 0.38));
            gizmos.circle_2d(pos.0, radius * 0.45, Color::srgb(0.75, 1.0, 0.78));
        }
    }
}

impl Plugin for FoodRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (FoodRenderPlugin::draw_food, sync_asset_food_visuals),
        );
    }
}

fn sync_asset_food_visuals(
    mut commands: Commands,
    config: Res<GameConfig>,
    sheet: Res<PowerlineSpriteSheet>,
    food: Query<(Entity, &Position), With<FoodMarker>>,
    mut visuals: Query<(Entity, &FoodVisual, &mut Transform, &mut Sprite)>,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut visuals {
            commands.entity(entity).despawn();
        }
        return;
    }

    let visual_size = (config.food.visual_radius.max(1.0) * 4.0).max(6.0);
    let mut seen = HashSet::with_capacity(food.iter().len());
    for (target, position) in &food {
        seen.insert(target);
        let transform = Transform::from_translation(position.0.extend(FOOD_Z));
        let sprite = sheet.sprite(
            PowerlineFrame::Food,
            Vec2::splat(visual_size),
            food_color(target),
        );

        let mut updated = false;
        for (_, visual, mut visual_transform, mut visual_sprite) in &mut visuals {
            if visual.target == target {
                *visual_transform = transform;
                *visual_sprite = sprite.clone();
                updated = true;
                break;
            }
        }
        if !updated {
            commands.spawn((FoodVisual { target }, sprite, transform));
        }
    }

    for (entity, visual, _, _) in &mut visuals {
        if !seen.contains(&visual.target) {
            commands.entity(entity).despawn();
        }
    }
}

fn food_color(entity: Entity) -> Color {
    let hue = (entity.to_bits() % 360) as f32;
    Color::hsl(hue, 1.0, 0.55)
}
