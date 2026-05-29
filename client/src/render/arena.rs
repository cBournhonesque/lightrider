use bevy::prelude::*;

use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;

pub(crate) struct ArenaRenderPlugin;

const BACKGROUND_Z: f32 = -100.0;
const BORDER_Z: f32 = -90.0;

impl Plugin for ArenaRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.015, 0.018, 0.022)));
        app.add_systems(Startup, spawn_asset_arena);
        app.add_systems(Update, draw_arena);
    }
}

fn spawn_asset_arena(
    mut commands: Commands,
    config: Res<GameConfig>,
    sheet: Res<PowerlineSpriteSheet>,
) {
    if !config.render.use_assets {
        return;
    }

    let half_width = config.arena.width * 0.5;
    let half_height = config.arena.height * 0.5;
    let tile_size = config.render.background_tile_size.max(16.0);
    let columns = (config.arena.width / tile_size).ceil() as i32 + 2;
    let rows = (config.arena.height / tile_size).ceil() as i32 + 2;
    let start_x = -columns as f32 * tile_size * 0.5 + tile_size * 0.5;
    let start_y = -rows as f32 * tile_size * 0.5 + tile_size * 0.5;

    for column in 0..columns {
        for row in 0..rows {
            let position = Vec3::new(
                start_x + column as f32 * tile_size,
                start_y + row as f32 * tile_size,
                BACKGROUND_Z,
            );
            commands.spawn((
                sheet.sprite(
                    PowerlineFrame::Grid,
                    Vec2::splat(tile_size),
                    Color::srgba(0.72, 0.9, 1.0, 0.72),
                ),
                Transform::from_translation(position),
            ));
        }
    }

    let outline_width = config.render.map_outline_width.max(1.0);
    let border_color = Color::srgba(0.68, 1.0, 1.0, 0.92);
    let horizontal_size = Vec2::new(config.arena.width, outline_width);
    let vertical_size = Vec2::new(config.arena.height, outline_width);
    for y in [-half_height, half_height] {
        commands.spawn((
            sheet.sprite(PowerlineFrame::WallStretch, horizontal_size, border_color),
            Transform::from_xyz(0.0, y, BORDER_Z),
        ));
    }
    for x in [-half_width, half_width] {
        commands.spawn((
            sheet.sprite(PowerlineFrame::WallStretch, vertical_size, border_color),
            Transform::from_xyz(x, 0.0, BORDER_Z)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ));
    }
}

fn draw_arena(mut gizmos: Gizmos, config: Res<GameConfig>) {
    if config.render.use_assets {
        return;
    }

    let half_width = config.arena.width * 0.5;
    let half_height = config.arena.height * 0.5;
    let min = Vec2::new(-half_width, -half_height);
    let max = Vec2::new(half_width, half_height);
    let border = Color::srgb(0.55, 0.62, 0.7);
    let outline = Color::srgb(0.05, 0.95, 1.0);
    let axis = Color::srgb(0.12, 0.16, 0.2);

    draw_rect(
        &mut gizmos,
        min,
        max,
        config.render.map_outline_width.max(1.0),
        outline,
    );
    draw_rect(&mut gizmos, min, max, 1.0, border);
    gizmos.line_2d(Vec2::new(min.x, 0.0), Vec2::new(max.x, 0.0), axis);
    gizmos.line_2d(Vec2::new(0.0, min.y), Vec2::new(0.0, max.y), axis);
    gizmos.circle_2d(Vec2::ZERO, 6.0, Color::srgb(0.25, 0.32, 0.38));
}

fn draw_rect(gizmos: &mut Gizmos, min: Vec2, max: Vec2, width: f32, color: Color) {
    let bottom_left = min;
    let bottom_right = Vec2::new(max.x, min.y);
    let top_right = max;
    let top_left = Vec2::new(min.x, max.y);

    draw_segment(gizmos, bottom_left, bottom_right, width, color);
    draw_segment(gizmos, bottom_right, top_right, width, color);
    draw_segment(gizmos, top_right, top_left, width, color);
    draw_segment(gizmos, top_left, bottom_left, width, color);
}

fn draw_segment(gizmos: &mut Gizmos, start: Vec2, end: Vec2, width: f32, color: Color) {
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
