use bevy::prelude::*;
use std::collections::HashSet;

use crate::food::ConfirmedFoodPickup;
use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct FoodRenderPlugin;

const FOOD_Z: f32 = 2.0;
const FOOD_GLOW_Z: f32 = FOOD_Z - 0.05;
const FOOD_PICKUP_ANIMATION_Z: f32 = 6.0;
const FOOD_PICKUP_ANIMATION_SECONDS: f32 = 0.34;
const FOOD_PULSE_SPEED: f32 = 3.35;
const FOOD_PULSE_AMPLITUDE: f32 = 0.075;
const FOOD_GLOW_PULSE_SPEED: f32 = 2.35;
const FOOD_GLOW_MIN_ALPHA: f32 = 0.16;
const FOOD_GLOW_MAX_ALPHA: f32 = 0.62;
const FOOD_GLOW_SCALE_BOOST: f32 = 0.28;
const FOOD_ROTATION_PERIOD_SECONDS: f32 = 5.0;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FoodVisual {
    target: Entity,
    layer: FoodVisualLayer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum FoodVisualLayer {
    Glow,
    Core,
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
        query: Query<(Entity, &Position, Option<&FoodColor>), With<FoodMarker>>,
    ) {
        if config.render.use_assets {
            return;
        }

        let radius = config.food.visual_radius.max(1.0);
        for (entity, pos, color) in query.iter() {
            let color = food_color(entity, color);
            let mut glow = color;
            glow.set_alpha(0.22);
            gizmos.circle_2d(pos.0, radius * 1.85, glow);
            gizmos.circle_2d(pos.0, radius * 1.15, color);
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
    food: Query<(Entity, &Position, Option<&FoodColor>), With<FoodMarker>>,
    mut visuals: Query<(Entity, &FoodVisual, &mut Transform, &mut Sprite)>,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut visuals {
            commands.entity(entity).despawn();
        }
        return;
    }

    let visual_size = food_visual_size(&config);
    let mut seen = HashSet::with_capacity(food.iter().len());
    for (target, position, tone) in &food {
        seen.insert(target);
        let pulse = food_pulse_scale(target, time.elapsed_secs());
        let glow_pulse = food_glow_pulse(target, time.elapsed_secs());
        let rotation = food_rotation(target, time.elapsed_secs());
        let color = food_color(target, tone);
        sync_food_visual_layer(
            &mut commands,
            &sheet,
            &mut visuals,
            target,
            FoodVisualLayer::Glow,
            PowerlineFrame::Glow,
            position.0,
            visual_size * 1.42,
            food_glow_color(color, glow_pulse),
            rotation,
            pulse * (1.04 + glow_pulse * FOOD_GLOW_SCALE_BOOST),
            FOOD_GLOW_Z,
        );
        sync_food_visual_layer(
            &mut commands,
            &sheet,
            &mut visuals,
            target,
            FoodVisualLayer::Core,
            PowerlineFrame::Food,
            position.0,
            visual_size,
            color,
            rotation,
            pulse,
            FOOD_Z,
        );
    }

    for (entity, visual, _, _) in &mut visuals {
        if !seen.contains(&visual.target) {
            commands.entity(entity).despawn();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_food_visual_layer(
    commands: &mut Commands,
    sheet: &PowerlineSpriteSheet,
    visuals: &mut Query<(Entity, &FoodVisual, &mut Transform, &mut Sprite)>,
    target: Entity,
    layer: FoodVisualLayer,
    frame: PowerlineFrame,
    position: Vec2,
    size: f32,
    color: Color,
    rotation: Quat,
    scale: f32,
    z: f32,
) {
    let transform = Transform::from_translation(position.extend(z))
        .with_rotation(rotation)
        .with_scale(Vec3::splat(scale));
    let sprite = sheet.sprite(frame, Vec2::splat(size), color);

    for (_, visual, mut visual_transform, mut visual_sprite) in visuals {
        if visual.target == target && visual.layer == layer {
            *visual_transform = transform;
            *visual_sprite = sprite;
            return;
        }
    }

    commands.spawn((FoodVisual { target, layer }, sprite, transform));
}

fn food_color(entity: Entity, tone: Option<&FoodColor>) -> Color {
    if let Some(tone) = tone {
        return Color::srgb(tone.red, tone.green, tone.blue);
    }

    let hue = (entity.to_bits() % 360) as f32;
    Color::hsl(hue, 1.0, 0.55)
}

fn food_glow_color(mut color: Color, pulse: f32) -> Color {
    color.set_alpha(lerp(
        FOOD_GLOW_MIN_ALPHA,
        FOOD_GLOW_MAX_ALPHA,
        pulse.clamp(0.0, 1.0),
    ));
    color
}

fn food_visual_size(config: &GameConfig) -> f32 {
    (config.food.visual_radius.max(0.25) * 3.65).max(2.0)
}

fn food_rotation(entity: Entity, elapsed_seconds: f32) -> Quat {
    Quat::from_rotation_z(food_rotation_angle(entity, elapsed_seconds))
}

fn food_rotation_angle(entity: Entity, elapsed_seconds: f32) -> f32 {
    let bits = entity.to_bits().wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let base = ((bits >> 40) as f32 / ((1_u64 << 24) as f32)) * std::f32::consts::TAU;
    base + elapsed_seconds * std::f32::consts::TAU / FOOD_ROTATION_PERIOD_SECONDS
}

fn food_pulse_scale(entity: Entity, elapsed_seconds: f32) -> f32 {
    let bits = entity.to_bits().wrapping_mul(0xd1b5_4a32_d192_ed03);
    let phase = ((bits >> 40) as f32 / ((1_u64 << 24) as f32)) * std::f32::consts::TAU;
    1.0 + (elapsed_seconds * FOOD_PULSE_SPEED + phase).sin() * FOOD_PULSE_AMPLITUDE
}

fn food_glow_pulse(entity: Entity, elapsed_seconds: f32) -> f32 {
    let bits = entity.to_bits().wrapping_mul(0x94d0_49bb_1331_11eb);
    let phase = ((bits >> 40) as f32 / ((1_u64 << 24) as f32)) * std::f32::consts::TAU;
    let wave = (elapsed_seconds * FOOD_GLOW_PULSE_SPEED + phase).sin() * 0.5 + 0.5;
    wave * wave
}

fn lerp(start: f32, end: f32, ratio: f32) -> f32 {
    start + (end - start) * ratio.clamp(0.0, 1.0)
}

fn spawn_confirmed_food_pickup_animations(
    mut commands: Commands,
    config: Res<GameConfig>,
    sheet: Res<PowerlineSpriteSheet>,
    mut pickups: MessageReader<ConfirmedFoodPickup>,
    snakes: Query<&SnakeHead>,
    food_colors: Query<&FoodColor>,
    visuals: Query<(Entity, &FoodVisual, &Sprite)>,
) {
    if !config.render.use_assets {
        for _ in pickups.read() {}
        return;
    }

    let visual_size = food_visual_size(&config);
    for pickup in pickups.read() {
        let mut color = food_colors
            .get(pickup.collision.food)
            .ok()
            .map(|tone| food_color(pickup.collision.food, Some(tone)))
            .unwrap_or_else(|| food_color(pickup.collision.food, None));
        for (visual_entity, visual, sprite) in &visuals {
            if visual.target == pickup.collision.food {
                if visual.layer == FoodVisualLayer::Core {
                    color = sprite.color;
                }
                commands.entity(visual_entity).try_despawn();
            }
        }
        let start = pickup.collision.food_position;
        let end = snakes
            .get(pickup.collision.snake)
            .map(|head| head.position)
            .unwrap_or(pickup.collision.head_position);
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
                .with_rotation(food_rotation(pickup.collision.food, 0.0)),
        ));
    }
}

fn update_food_pickup_animations(
    mut commands: Commands,
    time: Res<Time>,
    heads: Query<&SnakeHead>,
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

        let end = heads
            .get(animation.snake)
            .map(|head| head.position)
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
    fn food_glow_pulse_is_stronger_than_core_pulse() {
        let food = Entity::from_bits(42);
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for sample in 0..240 {
            let pulse = food_glow_pulse(food, sample as f32 / 30.0);
            min = min.min(pulse);
            max = max.max(pulse);
            assert!((0.0..=1.0).contains(&pulse));
        }
        assert!(max - min > 0.75);
    }

    #[test]
    fn food_rotation_completes_revolution_in_five_seconds() {
        let food = Entity::from_bits(42);
        let start = food_rotation_angle(food, 0.0);
        let end = food_rotation_angle(food, FOOD_ROTATION_PERIOD_SECONDS);
        assert!((end - start - std::f32::consts::TAU).abs() < 0.001);
    }

    #[test]
    fn food_color_is_stable_for_animation() {
        let food = Entity::from_bits(123);
        assert_eq!(food_color(food, None), food_color(food, None));
    }

    #[test]
    fn food_color_uses_replicated_tone_when_present() {
        let food = Entity::from_bits(123);
        let tone = FoodColor::new(0.1, 0.4, 0.8);

        assert_eq!(food_color(food, Some(&tone)), Color::srgb(0.1, 0.4, 0.8));
    }

    #[test]
    fn food_visual_size_stays_smaller_than_old_sprite_scale() {
        let config = GameConfig::default();
        assert!(food_visual_size(&config) < config.food.visual_radius * 4.0);
    }

    #[test]
    fn food_glow_uses_partial_alpha() {
        assert!(food_glow_color(Color::srgb(1.0, 0.0, 0.0), 1.0).alpha() < 1.0);
        assert!(
            food_glow_color(Color::srgb(1.0, 0.0, 0.0), 1.0).alpha()
                > food_glow_color(Color::srgb(1.0, 0.0, 0.0), 0.0).alpha()
        );
    }

    #[test]
    fn food_glow_stays_soft() {
        assert!(FOOD_GLOW_MAX_ALPHA <= 0.65);
        assert!(FOOD_GLOW_SCALE_BOOST <= 0.3);
        assert!(food_glow_color(Color::srgb(1.0, 0.0, 0.0), 1.0).alpha() <= 0.65);
    }

    #[test]
    fn food_glow_pulse_is_slower_than_core_pulse() {
        assert!(FOOD_GLOW_PULSE_SPEED < FOOD_PULSE_SPEED);
    }
}
