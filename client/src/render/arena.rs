use bevy::prelude::*;
use shared::config::GameConfig;

pub(crate) struct ArenaRenderPlugin;

const BACKGROUND_Z: f32 = -100.0;
const GRID_Z: f32 = -99.5;
const BORDER_GLOW_Z: f32 = -90.5;
const BORDER_Z: f32 = -90.0;
const BORDER_GLOW_ALPHA: f32 = 0.075;
const BORDER_CORE_ALPHA: f32 = 0.92;

impl Plugin for ArenaRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.018, 0.042, 0.064)));
        app.add_systems(Startup, spawn_asset_arena);
        app.add_systems(Update, draw_arena);
    }
}

fn spawn_asset_arena(mut commands: Commands, config: Res<GameConfig>) {
    if !config.render.use_assets {
        return;
    }

    let background = Sprite::from_color(
        Color::srgb(0.014, 0.046, 0.068),
        Vec2::new(config.arena.width, config.arena.height),
    );
    commands.spawn((background, Transform::from_xyz(0.0, 0.0, BACKGROUND_Z)));
    spawn_background_grid(&mut commands, &config);

    let outline_width = config.render.map_outline_width.max(1.0);
    let core_width = outline_width.clamp(1.15, 1.55);
    let glow_width = (outline_width * 5.0).max(12.0);
    spawn_inner_border_layer(
        &mut commands,
        config.arena.width,
        config.arena.height,
        glow_width,
        Color::linear_rgba(0.02, 0.46, 2.4, BORDER_GLOW_ALPHA),
        BORDER_GLOW_Z,
    );
    spawn_inner_border_layer(
        &mut commands,
        config.arena.width,
        config.arena.height,
        core_width,
        Color::linear_rgba(0.10, 0.92, 4.2, BORDER_CORE_ALPHA),
        BORDER_Z,
    );
}

fn spawn_background_grid(commands: &mut Commands, config: &GameConfig) {
    let half_width = config.arena.width * 0.5;
    let half_height = config.arena.height * 0.5;
    let tile_size = config.render.background_tile_size.max(32.0);
    let line_color = Color::srgb(0.025, 0.085, 0.12);
    let line_width = 1.0;

    let mut x = -half_width;
    while x <= half_width + f32::EPSILON {
        commands.spawn((
            Sprite::from_color(line_color, Vec2::new(line_width, config.arena.height)),
            Transform::from_xyz(x, 0.0, GRID_Z),
        ));
        x += tile_size;
    }

    let mut y = -half_height;
    while y <= half_height + f32::EPSILON {
        commands.spawn((
            Sprite::from_color(line_color, Vec2::new(config.arena.width, line_width)),
            Transform::from_xyz(0.0, y, GRID_Z),
        ));
        y += tile_size;
    }
}

fn draw_arena(mut gizmos: Gizmos, config: Res<GameConfig>) {
    if config.render.use_assets {
        return;
    }

    let half_width = config.arena.width * 0.5;
    let half_height = config.arena.height * 0.5;
    let glow_width = (config.render.map_outline_width.max(1.0) * 5.0).max(12.0);
    let core_width = config.render.map_outline_width.max(1.0).clamp(1.15, 1.55);
    let glow_min = Vec2::new(
        -half_width + glow_width * 0.5,
        -half_height + glow_width * 0.5,
    );
    let glow_max = Vec2::new(
        half_width - glow_width * 0.5,
        half_height - glow_width * 0.5,
    );
    let core_min = Vec2::new(
        -half_width + core_width * 0.5,
        -half_height + core_width * 0.5,
    );
    let core_max = Vec2::new(
        half_width - core_width * 0.5,
        half_height - core_width * 0.5,
    );
    let glow = Color::linear_rgba(0.02, 0.46, 2.4, BORDER_GLOW_ALPHA);
    let core = Color::linear_rgba(0.10, 0.92, 4.2, BORDER_CORE_ALPHA);

    draw_rect(&mut gizmos, glow_min, glow_max, glow_width, glow);
    draw_rect(&mut gizmos, core_min, core_max, core_width, core);
}

fn spawn_inner_border_layer(
    commands: &mut Commands,
    arena_width: f32,
    arena_height: f32,
    width: f32,
    color: Color,
    z: f32,
) {
    let half_width = arena_width * 0.5;
    let half_height = arena_height * 0.5;
    let horizontal_size = Vec2::new(arena_width, width);
    let vertical_size = Vec2::new(width, (arena_height - width * 2.0).max(width));
    let inset = width * 0.5;

    for y in [-half_height + inset, half_height - inset] {
        commands.spawn((
            Sprite::from_color(color, horizontal_size),
            Transform::from_xyz(0.0, y, z),
        ));
    }
    for x in [-half_width + inset, half_width - inset] {
        commands.spawn((
            Sprite::from_color(color, vertical_size),
            Transform::from_xyz(x, 0.0, z),
        ));
    }
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
