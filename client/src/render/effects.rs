use bevy::ecs::query::Or;
use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Controlled, Interpolated, Predicted, Replicated};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;
use shared::utils::geometry::ray_segment_intersection;
use std::collections::HashSet;

use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use crate::render::colors::{snake_color_for_fallback, snake_color_for_player, SnakePaletteColor};

pub(crate) struct EffectsRenderPlugin;

const BOOST_MARKER_Z: f32 = 14.0;
const BOOST_LIGHTNING_Z: f32 = 13.0;
const SPEED_PARTICLE_Z: f32 = 12.5;
const SPEED_PARTICLE_COUNT: usize = 8;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BoostVisual(BoostVisualPart);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BoostVisualPart {
    Lightning,
    Spark,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SpeedParticleVisual {
    snake: Entity,
    index: usize,
}

#[derive(Clone, Copy, Debug)]
struct BoostContact {
    head: Vec2,
    core: Vec2,
    marker: Vec2,
    distance: f32,
    other: Entity,
    lightning_active: bool,
    spark_active: bool,
}

struct DesiredParticle {
    key: SpeedParticleVisual,
    transform: Transform,
    sprite: Sprite,
}

impl Plugin for EffectsRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (sync_boost_marker, sync_speed_particles).after(FrameInterpolationSystems::Interpolate),
        );
    }
}

fn sync_boost_marker(
    mut commands: Commands,
    config: Res<GameConfig>,
    time: Res<Time>,
    sheet: Res<PowerlineSpriteSheet>,
    players: Query<&Player>,
    snakes: Query<
        (
            Entity,
            &TailPoints,
            &RoomId,
            Option<&Speed>,
            Option<&HasPlayer>,
            Has<Controlled>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    mut visuals: Query<(Entity, &BoostVisual, &mut Transform, &mut Sprite)>,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut visuals {
            commands.entity(entity).despawn();
        }
        return;
    }

    let contact = nearest_controlled_boost_contact(&config, &snakes);
    let spark_frame = spark_frame(time.elapsed_secs());
    let mut seen = HashSet::new();

    if let Some(contact) = contact {
        let delta = contact.head - contact.core;
        let angle = delta.y.atan2(delta.x) - std::f32::consts::FRAC_PI_2;
        let marker_size = config.render.head_size.max(4.0) * 1.45;
        let other_color = snake_entity_color(contact.other, &snakes, &players);
        let mut desired = Vec::with_capacity(2);
        if contact.lightning_active {
            desired.push((
                BoostVisualPart::Lightning,
                Transform::from_translation(
                    ((contact.head + contact.core) * 0.5).extend(BOOST_LIGHTNING_Z),
                )
                .with_rotation(Quat::from_rotation_z(angle)),
                sheet.sprite(
                    lightning_frame(time.elapsed_secs()),
                    Vec2::new(marker_size * 0.55, contact.distance.max(marker_size)),
                    other_color.lightning(),
                ),
            ));
        }
        if contact.spark_active {
            desired.push((
                BoostVisualPart::Spark,
                Transform::from_translation(contact.marker.extend(BOOST_MARKER_Z))
                    .with_rotation(Quat::from_rotation_z(angle)),
                sheet.sprite(
                    spark_frame,
                    Vec2::new(marker_size, marker_size * 0.72),
                    other_color.spark(),
                ),
            ));
        }

        for (part, transform, sprite) in desired {
            seen.insert(part);
            let mut updated = false;
            for (_, visual, mut visual_transform, mut visual_sprite) in &mut visuals {
                if visual.0 == part {
                    *visual_transform = transform;
                    *visual_sprite = sprite.clone();
                    updated = true;
                    break;
                }
            }
            if !updated {
                commands.spawn((BoostVisual(part), sprite, transform));
            }
        }
    }

    for (entity, visual, _, _) in &mut visuals {
        if !seen.contains(&visual.0) {
            commands.entity(entity).despawn();
        }
    }
}

fn sync_speed_particles(
    mut commands: Commands,
    config: Res<GameConfig>,
    time: Res<Time>,
    sheet: Res<PowerlineSpriteSheet>,
    snakes: Query<
        (Entity, &TailPoints, Option<&Speed>),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    mut visuals: Query<(Entity, &SpeedParticleVisual, &mut Transform, &mut Sprite)>,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut visuals {
            commands.entity(entity).despawn();
        }
        return;
    }

    let desired = desired_speed_particles(&config, time.elapsed_secs(), &sheet, &snakes);
    let mut seen = HashSet::with_capacity(desired.len());

    for desired in desired {
        seen.insert(desired.key);
        let mut updated = false;
        for (_, visual, mut transform, mut sprite) in &mut visuals {
            if *visual == desired.key {
                *transform = desired.transform;
                *sprite = desired.sprite.clone();
                updated = true;
                break;
            }
        }
        if !updated {
            commands.spawn((desired.key, desired.sprite, desired.transform));
        }
    }

    for (entity, visual, _, _) in &mut visuals {
        if !seen.contains(visual) {
            commands.entity(entity).despawn();
        }
    }
}

fn desired_speed_particles(
    config: &GameConfig,
    elapsed_seconds: f32,
    sheet: &PowerlineSpriteSheet,
    snakes: &Query<
        (Entity, &TailPoints, Option<&Speed>),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) -> Vec<DesiredParticle> {
    let mut desired = Vec::new();
    let min_speed = config.movement.min_speed;
    let max_speed = config.movement.max_speed.max(min_speed + f32::EPSILON);
    let threshold = min_speed + (max_speed - min_speed) * 0.58;
    let head_size = config.render.head_size.max(4.0);

    for (snake, tail, speed) in snakes {
        let speed = speed.map(|speed| speed.0).unwrap_or(min_speed);
        if speed < threshold {
            continue;
        }
        let speed_t = ((speed - threshold) / (max_speed - threshold)).clamp(0.0, 1.0);
        let head = tail.front().0;
        let direction = tail.front().1.delta();
        let normal = direction.perp();
        let particle_count =
            ((SPEED_PARTICLE_COUNT as f32) * (0.35 + speed_t * 0.65)).ceil() as usize;
        for index in 0..particle_count.min(SPEED_PARTICLE_COUNT) {
            let seed = index as f32 * 0.618_034 + snake.to_bits() as f32 * 0.000_013;
            let emission_rate = 7.0 + speed_t * 9.0;
            let age = (elapsed_seconds * emission_rate + seed).fract();
            let spread = (seed * std::f32::consts::TAU + elapsed_seconds * 0.7).sin();
            let behind = head_size * 0.55 + age * (head_size * 3.4 + speed_t * 22.0);
            let side = spread * (head_size * 0.28 + age * head_size * 0.75);
            let position = head - direction * behind + normal * side;
            let fade = (1.0 - age).powf(1.35);
            let size = (head_size * (0.62 + speed_t * 0.42) * (0.62 + 0.38 * fade)).max(4.5);
            let color = Color::srgba(0.74, 0.96, 1.0, fade * (0.22 + speed_t * 0.45));
            desired.push(DesiredParticle {
                key: SpeedParticleVisual { snake, index },
                transform: Transform::from_translation(position.extend(SPEED_PARTICLE_Z))
                    .with_rotation(direction_rotation(direction)),
                sprite: sheet.sprite(PowerlineFrame::ParticleDot, Vec2::splat(size), color),
            });
        }
    }

    desired
}

fn direction_rotation(direction: Vec2) -> Quat {
    Quat::from_rotation_z(direction.y.atan2(direction.x))
}

fn nearest_controlled_boost_contact(
    config: &GameConfig,
    snakes: &Query<
        (
            Entity,
            &TailPoints,
            &RoomId,
            Option<&Speed>,
            Option<&HasPlayer>,
            Has<Controlled>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) -> Option<BoostContact> {
    let max_distance = config.movement.boost_distance;
    if max_distance <= 0.0 {
        return None;
    }

    let mut nearest = None;
    for (entity, tail, room, speed, _, controlled) in snakes {
        if !controlled {
            continue;
        }
        let speed = speed
            .map(|speed| speed.0)
            .unwrap_or(config.movement.min_speed);
        let spark_active = speed >= top_speed_marker_threshold(config);
        let head = tail.front().0;
        let direction = tail.front().1.delta();
        let lightning_min_distance = config.render.head_size.max(4.0) * 1.6;
        let core_radius = config.render.tail_width.max(1.25) * 0.5;
        let left = nearest_tail_ray_hit(
            head,
            direction.perp(),
            direction,
            max_distance,
            lightning_min_distance,
            core_radius,
            entity,
            room,
            snakes,
            spark_active,
        );
        let right = nearest_tail_ray_hit(
            head,
            -direction.perp(),
            direction,
            max_distance,
            lightning_min_distance,
            core_radius,
            entity,
            room,
            snakes,
            spark_active,
        );
        let contact = match (left, right) {
            (Some(left), Some(right)) => Some(if left.distance <= right.distance {
                left
            } else {
                right
            }),
            (Some(contact), None) | (None, Some(contact)) => Some(contact),
            (None, None) => None,
        };
        if let Some(contact) = contact {
            if nearest.map_or(true, |nearest: BoostContact| {
                contact.distance < nearest.distance
            }) {
                nearest = Some(contact);
            }
        }
    }
    nearest
}

fn nearest_tail_ray_hit(
    origin: Vec2,
    direction: Vec2,
    forward: Vec2,
    max_distance: f32,
    lightning_min_distance: f32,
    core_radius: f32,
    excluded: Entity,
    room: &RoomId,
    snakes: &Query<
        (
            Entity,
            &TailPoints,
            &RoomId,
            Option<&Speed>,
            Option<&HasPlayer>,
            Has<Controlled>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    spark_active: bool,
) -> Option<BoostContact> {
    let mut nearest = None;
    for (other_entity, other_tail, other_room, _, _, _) in snakes {
        if other_entity == excluded || other_room != room {
            continue;
        }
        for (segment_start, segment_end) in other_tail.pairs_front_to_back() {
            let segment = segment_end.0 - segment_start.0;
            let segment_length = segment.length();
            if segment_length <= f32::EPSILON {
                continue;
            }
            let segment_direction = segment / segment_length;
            if segment_direction.dot(forward).abs() < 0.97 {
                continue;
            }
            let Some(distance) = ray_segment_intersection(
                origin,
                direction,
                max_distance,
                segment_start.0,
                segment_end.0,
            ) else {
                continue;
            };
            if nearest.map_or(true, |nearest: BoostContact| distance < nearest.distance) {
                let hit = origin + direction * distance;
                let lightning_active = distance >= lightning_min_distance;
                nearest = Some(BoostContact {
                    head: origin,
                    core: hit,
                    marker: hit - direction * core_radius,
                    distance,
                    other: other_entity,
                    lightning_active,
                    spark_active: spark_active || !lightning_active,
                });
            }
        }
    }
    nearest
}

fn top_speed_marker_threshold(config: &GameConfig) -> f32 {
    let min_speed = config.movement.min_speed;
    let max_speed = config.movement.max_speed.max(min_speed);
    min_speed + (max_speed - min_speed) * 0.92
}

fn snake_entity_color(
    snake_entity: Entity,
    snakes: &Query<
        (
            Entity,
            &TailPoints,
            &RoomId,
            Option<&Speed>,
            Option<&HasPlayer>,
            Has<Controlled>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    players: &Query<&Player>,
) -> SnakePaletteColor {
    snakes
        .iter()
        .find_map(|(entity, _, _, _, has_player, _)| {
            (entity == snake_entity).then(|| {
                has_player
                    .and_then(|has_player| players.get(has_player.0).ok())
                    .map(snake_color_for_player)
                    .unwrap_or_else(|| snake_color_for_fallback(entity.to_bits()))
            })
        })
        .unwrap_or_else(|| snake_color_for_fallback(snake_entity.to_bits()))
}

fn spark_frame(elapsed_seconds: f32) -> PowerlineFrame {
    match ((elapsed_seconds * 18.0) as usize) % 3 {
        0 => PowerlineFrame::Spark0,
        1 => PowerlineFrame::Spark1,
        _ => PowerlineFrame::Spark2,
    }
}

fn lightning_frame(elapsed_seconds: f32) -> PowerlineFrame {
    match ((elapsed_seconds * 14.0) as usize) % 3 {
        0 => PowerlineFrame::Lightning1,
        1 => PowerlineFrame::Lightning2,
        _ => PowerlineFrame::Lightning3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spark_frames_cycle() {
        assert_eq!(spark_frame(0.0), PowerlineFrame::Spark0);
        assert_eq!(spark_frame(1.0 / 18.0), PowerlineFrame::Spark1);
        assert_eq!(spark_frame(2.0 / 18.0), PowerlineFrame::Spark2);
    }

    #[test]
    fn top_speed_marker_threshold_is_near_max_speed() {
        let config = GameConfig::default();

        assert!(top_speed_marker_threshold(&config) > 3.7);
        assert!(top_speed_marker_threshold(&config) < config.movement.max_speed);
    }
}
