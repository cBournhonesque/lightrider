use bevy::prelude::*;

use shared::config::GameConfig;

pub(crate) struct ArenaRenderPlugin;

impl Plugin for ArenaRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.015, 0.018, 0.022)));
        app.add_systems(Update, draw_arena);
    }
}

fn draw_arena(mut gizmos: Gizmos, config: Res<GameConfig>) {
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
