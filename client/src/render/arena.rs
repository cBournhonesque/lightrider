use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;

use shared::config::GameConfig;

pub(crate) struct ArenaRenderPlugin;

const BACKGROUND_Z: f32 = -100.0;
const GRID_Z: f32 = -99.5;
const BORDER_GLOW_Z: f32 = -90.5;
const BORDER_Z: f32 = -90.0;

impl Plugin for ArenaRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.018, 0.042, 0.064)));
        app.add_systems(Startup, spawn_asset_arena);
        app.add_systems(Update, draw_arena);
    }
}

fn spawn_asset_arena(
    mut commands: Commands,
    config: Res<GameConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    if !config.render.use_assets {
        return;
    }

    let half_width = config.arena.width * 0.5;
    let half_height = config.arena.height * 0.5;
    let background = Sprite::from_color(
        Color::srgb(0.014, 0.046, 0.068),
        Vec2::new(config.arena.width, config.arena.height),
    );
    commands.spawn((background, Transform::from_xyz(0.0, 0.0, BACKGROUND_Z)));
    spawn_background_grid(&mut commands, &config);

    let outline_width = config.render.map_outline_width.max(1.0);
    let glow_width = (outline_width * 8.0).max(18.0);
    let glow_material = materials.add(ColorMaterial {
        color: Color::linear_rgba(0.04, 0.45, 1.45, 0.10),
        alpha_mode: AlphaMode2d::Blend,
        ..default()
    });
    let border_material = materials.add(ColorMaterial {
        color: Color::linear_rgb(0.08, 0.9, 2.45),
        alpha_mode: AlphaMode2d::Opaque,
        ..default()
    });
    let horizontal_glow_mesh = meshes.add(Capsule2d::new(glow_width * 0.5, config.arena.width));
    let vertical_glow_mesh = meshes.add(Capsule2d::new(glow_width * 0.5, config.arena.height));
    let horizontal_mesh = meshes.add(Capsule2d::new(outline_width * 0.5, config.arena.width));
    let vertical_mesh = meshes.add(Capsule2d::new(outline_width * 0.5, config.arena.height));
    for y in [-half_height, half_height] {
        commands.spawn((
            Mesh2d(horizontal_glow_mesh.clone()),
            MeshMaterial2d(glow_material.clone()),
            Transform::from_xyz(0.0, y, BORDER_GLOW_Z)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ));
        commands.spawn((
            Mesh2d(horizontal_mesh.clone()),
            MeshMaterial2d(border_material.clone()),
            Transform::from_xyz(0.0, y, BORDER_Z)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ));
    }
    for x in [-half_width, half_width] {
        commands.spawn((
            Mesh2d(vertical_glow_mesh.clone()),
            MeshMaterial2d(glow_material.clone()),
            Transform::from_xyz(x, 0.0, BORDER_GLOW_Z),
        ));
        commands.spawn((
            Mesh2d(vertical_mesh.clone()),
            MeshMaterial2d(border_material.clone()),
            Transform::from_xyz(x, 0.0, BORDER_Z),
        ));
    }
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
