use bevy::ecs::query::Or;
use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Interpolated, Predicted, Replicated};
use std::collections::HashSet;

use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct SnakeRenderPlugin;

const SNAKE_TAIL_Z: f32 = 10.0;
const SNAKE_HEAD_Z: f32 = 11.0;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SnakeVisual {
    key: SnakeVisualKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SnakeVisualKey {
    owner: Entity,
    part: SnakeVisualPart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SnakeVisualPart {
    Head,
    Segment(usize),
}

struct DesiredSnakeVisual {
    key: SnakeVisualKey,
    transform: Transform,
    sprite: Sprite,
}

impl Plugin for SnakeRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (draw_snakes, sync_asset_snake_visuals).after(FrameInterpolationSystems::Interpolate),
        );
    }
}

/// Draw predicted/local snakes, interpolated remote snakes, and server-owned snakes.
pub(crate) fn draw_snakes(
    mut gizmos: Gizmos,
    config: Res<GameConfig>,
    tails: Query<&TailPoints, Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>>,
) {
    if config.render.use_assets {
        return;
    }

    let color = Color::srgb(0.1, 0.75, 1.0);
    let head_color = Color::srgb(0.75, 0.95, 1.0);
    let tail_width = config.render.tail_width.max(1.0);
    let head_size = config.render.head_size.max(1.0);
    for points in tails.iter() {
        gizmos.rect_2d(points.front().0, Vec2::ONE * head_size, head_color);
        points.pairs_front_to_back().for_each(|(start, end)| {
            draw_tail_segment(&mut gizmos, start.0, end.0, tail_width, color);
            if start.0.x != end.0.x && start.0.y != end.0.y {
                info!("DIAGONAL");
            }
        });
    }
}

fn sync_asset_snake_visuals(
    mut commands: Commands,
    config: Res<GameConfig>,
    sheet: Res<PowerlineSpriteSheet>,
    tails: Query<
        (Entity, &TailPoints),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    mut visuals: Query<(Entity, &SnakeVisual, &mut Transform, &mut Sprite)>,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut visuals {
            commands.entity(entity).despawn();
        }
        return;
    }

    let desired = desired_snake_visuals(&config, &sheet, &tails);
    let mut seen = HashSet::with_capacity(desired.len());

    for desired in desired {
        seen.insert(desired.key);
        let mut updated = false;
        for (_, visual, mut transform, mut sprite) in &mut visuals {
            if visual.key == desired.key {
                *transform = desired.transform;
                *sprite = desired.sprite.clone();
                updated = true;
                break;
            }
        }
        if !updated {
            commands.spawn((
                SnakeVisual { key: desired.key },
                desired.sprite,
                desired.transform,
            ));
        }
    }

    for (entity, visual, _, _) in &mut visuals {
        if !seen.contains(&visual.key) {
            commands.entity(entity).despawn();
        }
    }
}

fn desired_snake_visuals(
    config: &GameConfig,
    sheet: &PowerlineSpriteSheet,
    tails: &Query<
        (Entity, &TailPoints),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) -> Vec<DesiredSnakeVisual> {
    let tail_color = Color::srgba(0.1, 0.75, 1.0, 0.95);
    let head_color = Color::srgb(0.82, 0.98, 1.0);
    let tail_width = config.render.tail_width.max(1.0);
    let head_size = config.render.head_size.max(1.0) * 1.8;
    let mut desired = Vec::new();

    for (owner, points) in tails.iter() {
        desired.push(DesiredSnakeVisual {
            key: SnakeVisualKey {
                owner,
                part: SnakeVisualPart::Head,
            },
            transform: Transform::from_translation(points.front().0.extend(SNAKE_HEAD_Z)),
            sprite: sheet.sprite(PowerlineFrame::HeadDot, Vec2::splat(head_size), head_color),
        });

        for (index, (start, end)) in points.pairs_front_to_back().enumerate() {
            let delta = end.0 - start.0;
            let length = delta.length();
            if length <= f32::EPSILON {
                continue;
            }
            let center = (start.0 + end.0) * 0.5;
            desired.push(DesiredSnakeVisual {
                key: SnakeVisualKey {
                    owner,
                    part: SnakeVisualPart::Segment(index),
                },
                transform: Transform::from_translation(center.extend(SNAKE_TAIL_Z))
                    .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x))),
                sprite: sheet.sprite(
                    PowerlineFrame::WallStretch,
                    Vec2::new(length + tail_width, tail_width),
                    tail_color,
                ),
            });
        }
    }

    desired
}

fn draw_tail_segment(gizmos: &mut Gizmos, start: Vec2, end: Vec2, width: f32, color: Color) {
    let delta = end - start;
    let normal = if delta.length_squared() > f32::EPSILON {
        Vec2::new(-delta.y, delta.x).normalize()
    } else {
        Vec2::ZERO
    };
    let line_count = width.round().max(1.0) as i32;
    let center = (line_count - 1) as f32 * 0.5;
    for line in 0..line_count {
        let offset = normal * (line as f32 - center);
        gizmos.line_2d(start + offset, end + offset, color);
    }
}
