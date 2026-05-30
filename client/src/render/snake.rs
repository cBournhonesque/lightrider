use bevy::ecs::query::Or;
use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Interpolated, Predicted, Replicated};
use std::collections::HashSet;

use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct SnakeRenderPlugin;

const SNAKE_HEAD_Z: f32 = 11.0;
const SNAKE_GLOW_Z: f32 = 8.0;
const SNAKE_GLOW_LAYERS: [GlowLayer; 3] = [
    GlowLayer {
        id: 0,
        width_multiplier: 6.0,
        alpha: 0.08,
        z_offset: 0.0,
    },
    GlowLayer {
        id: 1,
        width_multiplier: 3.0,
        alpha: 0.2,
        z_offset: 0.5,
    },
    GlowLayer {
        id: 2,
        width_multiplier: 1.0,
        alpha: 0.95,
        z_offset: 1.0,
    },
];

#[derive(Clone, Copy)]
struct GlowLayer {
    id: u8,
    width_multiplier: f32,
    alpha: f32,
    z_offset: f32,
}

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
    Head(u8),
    Segment { index: usize, layer: u8 },
    Joint { index: usize, layer: u8 },
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
    let tail_rgb = Vec3::new(0.08, 0.78, 1.0);
    let head_rgb = Vec3::new(0.82, 0.98, 1.0);
    let tail_width = config.render.tail_width.max(1.0);
    let head_size = config.render.head_size.max(tail_width * 1.8);
    let mut desired = Vec::new();

    for (owner, points) in tails.iter() {
        let head = points.front().0;
        for layer in SNAKE_GLOW_LAYERS {
            let is_core = layer.id == 2;
            let size = if is_core {
                head_size
            } else {
                (head_size * layer.width_multiplier * 0.85).max(tail_width * 4.0)
            };
            desired.push(DesiredSnakeVisual {
                key: SnakeVisualKey {
                    owner,
                    part: SnakeVisualPart::Head(layer.id),
                },
                transform: Transform::from_translation(head.extend(if is_core {
                    SNAKE_HEAD_Z
                } else {
                    SNAKE_GLOW_Z + 2.0
                })),
                sprite: sheet.sprite(
                    PowerlineFrame::HeadDot,
                    Vec2::splat(size),
                    rgba(if is_core { head_rgb } else { tail_rgb }, layer.alpha),
                ),
            });
        }

        for (index, (start, end)) in points.pairs_front_to_back().enumerate() {
            let delta = end.0 - start.0;
            let length = delta.length();
            if length <= f32::EPSILON {
                continue;
            }
            let center = (start.0 + end.0) * 0.5;
            for layer in SNAKE_GLOW_LAYERS {
                let width = tail_width * layer.width_multiplier;
                desired.push(DesiredSnakeVisual {
                    key: SnakeVisualKey {
                        owner,
                        part: SnakeVisualPart::Segment {
                            index,
                            layer: layer.id,
                        },
                    },
                    transform: Transform::from_translation(
                        center.extend(SNAKE_GLOW_Z + layer.z_offset),
                    )
                    .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x))),
                    sprite: sheet.sprite(
                        PowerlineFrame::WallStretch,
                        Vec2::new(length + width * 1.5, width),
                        rgba(tail_rgb, layer.alpha),
                    ),
                });
            }
        }

        for (index, point) in points.0.iter().enumerate() {
            for layer in SNAKE_GLOW_LAYERS {
                let width = tail_width * layer.width_multiplier;
                desired.push(DesiredSnakeVisual {
                    key: SnakeVisualKey {
                        owner,
                        part: SnakeVisualPart::Joint {
                            index,
                            layer: layer.id,
                        },
                    },
                    transform: Transform::from_translation(
                        point.0.extend(SNAKE_GLOW_Z + layer.z_offset),
                    ),
                    sprite: sheet.sprite(
                        PowerlineFrame::HeadDot,
                        Vec2::splat(width),
                        rgba(tail_rgb, layer.alpha),
                    ),
                });
            }
        }
    }

    desired
}

fn rgba(rgb: Vec3, alpha: f32) -> Color {
    Color::srgba(rgb.x, rgb.y, rgb.z, alpha)
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
