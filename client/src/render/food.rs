use bevy::prelude::*;
use std::collections::HashSet;

use crate::food::ConfirmedFoodPickup;
use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct FoodRenderPlugin;

const FOOD_Z: f32 = 2.0;
const FOOD_PICKUP_ANIMATION_Z: f32 = 6.0;
const FOOD_PICKUP_ANIMATION_SECONDS: f32 = 0.34;
const FOOD_PULSE_SPEED: f32 = 3.0;
const FOOD_PULSE_AMPLITUDE: f32 = 0.045;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FoodVisual {
    target: Entity,
}

#[derive(Component, Clone, Copy, Debug)]
struct FoodPickupAnimation {
    elapsed: f32,
    snake: Entity,
    start: Vec2,
    fallback_end: Vec2,
    color: Color,
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
            (
                FoodRenderPlugin::draw_food,
                sync_asset_food_visuals,
                spawn_confirmed_food_pickup_animations,
                update_food_pickup_animations,
            ),
        );
    }
}

fn sync_asset_food_visuals(
    mut commands: Commands,
    config: Res<GameConfig>,
    sheet: Res<PowerlineSpriteSheet>,
    time: Res<Time>,
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
        let transform = Transform::from_translation(position.0.extend(FOOD_Z))
            .with_rotation(food_rotation(target))
            .with_scale(Vec3::splat(food_pulse_scale(target, time.elapsed_secs())));
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

fn food_rotation(entity: Entity) -> Quat {
    let bits = entity.to_bits().wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let angle = ((bits >> 40) as f32 / ((1_u64 << 24) as f32)) * std::f32::consts::TAU;
    Quat::from_rotation_z(angle)
}

fn food_pulse_scale(entity: Entity, elapsed_seconds: f32) -> f32 {
    let bits = entity.to_bits().wrapping_mul(0xd1b5_4a32_d192_ed03);
    let phase = ((bits >> 40) as f32 / ((1_u64 << 24) as f32)) * std::f32::consts::TAU;
    1.0 + (elapsed_seconds * FOOD_PULSE_SPEED + phase).sin() * FOOD_PULSE_AMPLITUDE
}

fn spawn_confirmed_food_pickup_animations(
    mut commands: Commands,
    config: Res<GameConfig>,
    sheet: Res<PowerlineSpriteSheet>,
    mut pickups: MessageReader<ConfirmedFoodPickup>,
    snakes: Query<&TailPoints>,
    visuals: Query<(Entity, &FoodVisual)>,
) {
    if !config.render.use_assets {
        for _ in pickups.read() {}
        return;
    }

    let visual_size = (config.food.visual_radius.max(1.0) * 4.0).max(6.0);
    for pickup in pickups.read() {
        for (visual_entity, visual) in &visuals {
            if visual.target == pickup.collision.food {
                commands.entity(visual_entity).try_despawn();
            }
        }
        let start = pickup.collision.food_position;
        let end = snakes
            .get(pickup.collision.snake)
            .map(|tail| tail.front().0)
            .unwrap_or(pickup.collision.head_position);
        let color = food_color(pickup.collision.food);
        commands.spawn((
            FoodPickupAnimation {
                elapsed: 0.0,
                snake: pickup.collision.snake,
                start,
                fallback_end: end,
                color,
            },
            sheet.sprite(PowerlineFrame::Food, Vec2::splat(visual_size), color),
            Transform::from_translation(start.extend(FOOD_PICKUP_ANIMATION_Z))
                .with_rotation(food_rotation(pickup.collision.food)),
        ));
    }
}

fn update_food_pickup_animations(
    mut commands: Commands,
    time: Res<Time>,
    tails: Query<&TailPoints>,
    mut animations: Query<(
        Entity,
        &mut FoodPickupAnimation,
        &mut Transform,
        &mut Sprite,
    )>,
) {
    for (entity, mut animation, mut transform, mut sprite) in &mut animations {
        animation.elapsed += time.delta_secs();
        let t = (animation.elapsed / FOOD_PICKUP_ANIMATION_SECONDS).clamp(0.0, 1.0);
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let end = tails
            .get(animation.snake)
            .map(|tail| tail.front().0)
            .unwrap_or(animation.fallback_end);
        let eased = t * t;
        transform.translation = animation
            .start
            .lerp(end, eased)
            .extend(FOOD_PICKUP_ANIMATION_Z);
        transform.rotation = Quat::from_rotation_z(t * std::f32::consts::TAU * 1.5);
        transform.scale = Vec3::splat((1.0 - 0.65 * t).max(0.25));
        sprite.color = animation.color;
        sprite.color.set_alpha(1.0 - 0.35 * t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_pulse_stays_subtle() {
        let food = Entity::from_bits(42);
        for sample in 0..120 {
            let scale = food_pulse_scale(food, sample as f32 / 30.0);
            assert!(scale >= 1.0 - FOOD_PULSE_AMPLITUDE - f32::EPSILON);
            assert!(scale <= 1.0 + FOOD_PULSE_AMPLITUDE + f32::EPSILON);
        }
    }

    #[test]
    fn food_color_is_stable_for_animation() {
        let food = Entity::from_bits(123);
        assert_eq!(food_color(food), food_color(food));
    }
}
