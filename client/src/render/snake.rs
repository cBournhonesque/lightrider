use bevy::asset::RenderAssetUsages;
use bevy::ecs::query::Or;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Interpolated, Predicted, Replicated};
use std::collections::HashSet;

use crate::collision::death::ConfirmedDeath;
use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use crate::render::colors::{snake_color_for_fallback, snake_color_for_player, SnakePaletteColor};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct SnakeRenderPlugin;

const SNAKE_HEAD_Z: f32 = 11.0;
const SNAKE_HEAD_GLOW_Z: f32 = 10.8;
const SNAKE_TAIL_Z: f32 = 10.0;
const SNAKE_DEATH_Z: f32 = 16.0;
const SNAKE_DEATH_ANIMATION_SECONDS: f32 = 0.58;
const MESH_CURVE_SEGMENTS: u32 = 14;
const WIDTH_GROWTH_START_LENGTH: f32 = 2500.0;
const WIDTH_GROWTH_MAX_LENGTH: f32 = 5000.0;
const WIDTH_GROWTH_MAX_SCALE: f32 = 2.6;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SnakeVisual {
    key: SnakeVisualKey,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SnakeSpriteVisual;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SnakeMeshVisual;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SnakeVisualKey {
    owner: Entity,
    part: SnakeVisualPart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SnakeVisualPart {
    Head,
    HeadGlow,
    Segment { index: usize, layer: TailLayer },
    Joint { index: usize, layer: TailLayer },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TailLayer {
    Core,
}

struct DesiredSnakeSpriteVisual {
    key: SnakeVisualKey,
    transform: Transform,
    sprite: Sprite,
}

struct DesiredSnakeMeshVisual {
    key: SnakeVisualKey,
    transform: Transform,
    shape: TailMeshShape,
    color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HeadGlowVisual {
    diameter: f32,
    alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TailMeshShape {
    Capsule { length: f32, width: f32 },
    Circle { radius: f32 },
}

#[derive(Component, Clone, Copy, Debug)]
struct SnakeDeathVisual {
    elapsed: f32,
}

impl Plugin for SnakeRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                spawn_snake_death_animations,
                update_snake_death_animations.after(spawn_snake_death_animations),
            ),
        );
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
    tails: Query<
        (&SnakeHead, &TailPoints, Option<&TailLength>),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) {
    if config.render.use_assets {
        return;
    }

    let color = Color::srgb(0.1, 0.75, 1.0);
    let head_color = Color::srgb(0.75, 0.95, 1.0);
    for (head, points, length) in tails.iter() {
        let width_scale = snake_width_scale(length);
        let tail_width = config.render.tail_width.max(1.0) * width_scale;
        let head_size = config.render.head_size.max(1.0) * width_scale;
        let points = visible_tail(head, points, length);
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
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    players: Query<&Player>,
    tails: Query<
        (
            Entity,
            &SnakeHead,
            &TailPoints,
            Option<&TailLength>,
            Option<&Speed>,
            Option<&Acceleration>,
            Option<&HasPlayer>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    mut sprite_visuals: Query<
        (Entity, &SnakeVisual, &mut Transform, &mut Sprite),
        (With<SnakeSpriteVisual>, Without<SnakeMeshVisual>),
    >,
    mut mesh_visuals: Query<
        (
            Entity,
            &SnakeVisual,
            &mut Transform,
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
        ),
        (With<SnakeMeshVisual>, Without<SnakeSpriteVisual>),
    >,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut sprite_visuals {
            commands.entity(entity).despawn();
        }
        for (entity, _, _, mesh, material) in &mut mesh_visuals {
            mesh_assets.remove(mesh.0.id());
            materials.remove(material.0.id());
            commands.entity(entity).despawn();
        }
        return;
    }

    let (desired_sprites, desired_meshes) =
        desired_snake_visuals(&config, &sheet, &players, &tails);
    let mut seen_sprites = HashSet::with_capacity(desired_sprites.len());
    let mut seen_meshes = HashSet::with_capacity(desired_meshes.len());

    for desired in desired_sprites {
        seen_sprites.insert(desired.key);
        let mut updated = false;
        for (_, visual, mut transform, mut sprite) in &mut sprite_visuals {
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
                SnakeSpriteVisual,
                desired.sprite,
                desired.transform,
            ));
        }
    }

    for desired in desired_meshes {
        seen_meshes.insert(desired.key);
        let mut updated = false;
        for (_, visual, mut transform, mesh, material) in &mut mesh_visuals {
            if visual.key == desired.key {
                *transform = desired.transform;
                if let Some(mesh_asset) = mesh_assets.get_mut(&mesh.0) {
                    *mesh_asset = desired.shape.mesh();
                }
                set_material_color(&mut materials, material, desired.color);
                updated = true;
                break;
            }
        }
        if !updated {
            commands.spawn((
                SnakeVisual { key: desired.key },
                SnakeMeshVisual,
                Mesh2d(mesh_assets.add(desired.shape.mesh())),
                MeshMaterial2d(materials.add(blended_material(desired.color))),
                desired.transform,
            ));
        }
    }

    for (entity, visual, _, _) in &mut sprite_visuals {
        if !seen_sprites.contains(&visual.key) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, visual, _, mesh, material) in &mut mesh_visuals {
        if !seen_meshes.contains(&visual.key) {
            mesh_assets.remove(mesh.0.id());
            materials.remove(material.0.id());
            commands.entity(entity).despawn();
        }
    }
}

fn desired_snake_visuals(
    config: &GameConfig,
    sheet: &PowerlineSpriteSheet,
    players: &Query<&Player>,
    tails: &Query<
        (
            Entity,
            &SnakeHead,
            &TailPoints,
            Option<&TailLength>,
            Option<&Speed>,
            Option<&Acceleration>,
            Option<&HasPlayer>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) -> (Vec<DesiredSnakeSpriteVisual>, Vec<DesiredSnakeMeshVisual>) {
    let base_tail_width = config.render.tail_width.max(1.0);
    let base_head_size = config.render.head_size.max(base_tail_width * 1.8);
    let mut sprite_desired = Vec::new();
    let mut mesh_desired = Vec::new();

    for (owner, head, points, length, speed, acceleration, player) in tails.iter() {
        let width_scale = snake_width_scale(length);
        let tail_width = base_tail_width * width_scale;
        let head_size = base_head_size * width_scale;
        let head_diameter = (head_size * 0.62).max(tail_width * 2.5);
        let points = visible_tail(head, points, length);
        let color = snake_visual_color(owner, player, players);
        let head = points.front().0;
        let head_glow = head_glow_visual(head_diameter, speed, acceleration, config);
        sprite_desired.push(DesiredSnakeSpriteVisual {
            key: SnakeVisualKey {
                owner,
                part: SnakeVisualPart::HeadGlow,
            },
            transform: Transform::from_translation(head.extend(SNAKE_HEAD_GLOW_Z)),
            sprite: sheet.sprite(
                PowerlineFrame::HeadDot,
                Vec2::splat(head_glow.diameter),
                color.head_glow(head_glow.alpha),
            ),
        });
        sprite_desired.push(DesiredSnakeSpriteVisual {
            key: SnakeVisualKey {
                owner,
                part: SnakeVisualPart::Head,
            },
            transform: Transform::from_translation(head.extend(SNAKE_HEAD_Z))
                .with_rotation(direction_rotation(points.front().1)),
            sprite: sheet.sprite(
                PowerlineFrame::HeadDot,
                Vec2::splat(head_diameter),
                Color::linear_rgb(3.2, 3.2, 3.2),
            ),
        });

        for (index, (start, end)) in points.pairs_front_to_back().enumerate() {
            let delta = end.0 - start.0;
            let length = delta.length();
            if length <= f32::EPSILON {
                continue;
            }
            let rotation = Quat::from_rotation_z(delta.y.atan2(delta.x));
            for layer in TailLayer::ALL {
                let Some((center, length)) =
                    layer.segment_center_and_length(start.0, end.0, index, head_diameter)
                else {
                    continue;
                };
                let layer_width = layer.width(tail_width);
                mesh_desired.push(DesiredSnakeMeshVisual {
                    key: SnakeVisualKey {
                        owner,
                        part: SnakeVisualPart::Segment { index, layer },
                    },
                    transform: Transform::from_translation(center.extend(layer.z()))
                        .with_rotation(rotation),
                    shape: TailMeshShape::Capsule {
                        length: length.max(layer_width),
                        width: layer_width,
                    },
                    color: layer.color(color),
                });
            }
        }

        let joint_count = points.0.len().saturating_sub(1);
        for (index, (point, _)) in points
            .0
            .iter()
            .enumerate()
            .skip(1)
            .take(joint_count.saturating_sub(1))
        {
            for layer in TailLayer::ALL {
                let layer_width = layer.width(tail_width);
                mesh_desired.push(DesiredSnakeMeshVisual {
                    key: SnakeVisualKey {
                        owner,
                        part: SnakeVisualPart::Joint { index, layer },
                    },
                    transform: Transform::from_translation((*point).extend(layer.z())),
                    shape: TailMeshShape::Circle {
                        radius: layer_width * 0.5,
                    },
                    color: layer.color(color),
                });
            }
        }
    }

    (sprite_desired, mesh_desired)
}

fn head_glow_visual(
    head_diameter: f32,
    speed: Option<&Speed>,
    acceleration: Option<&Acceleration>,
    config: &GameConfig,
) -> HeadGlowVisual {
    let base_diameter = head_diameter * 3.0;
    let acceleration = acceleration
        .map(|acceleration| acceleration.0)
        .unwrap_or(0.0);
    if acceleration <= 0.0 {
        return HeadGlowVisual {
            diameter: base_diameter,
            alpha: 0.0,
        };
    }

    let speed = speed
        .map(|speed| speed.0)
        .unwrap_or(config.movement.min_speed);
    let speed_t = normalized_range(
        speed,
        config.movement.min_speed,
        config
            .movement
            .max_speed
            .max(config.movement.min_speed + f32::EPSILON),
    );
    if speed_t <= f32::EPSILON {
        return HeadGlowVisual {
            diameter: base_diameter,
            alpha: 0.0,
        };
    }
    let typical_acceleration = (config.movement.base_acceleration.abs()
        * config.movement.boost_acceleration_ratio
        + config.movement.food_boost_acceleration * 2.0)
        .max(0.01);
    let acceleration_t = (acceleration / typical_acceleration).clamp(0.0, 1.0);

    HeadGlowVisual {
        diameter: base_diameter * (1.0 + speed_t * 0.22 + acceleration_t * 0.10),
        alpha: 0.05 + speed_t * 0.06 + acceleration_t * 0.03,
    }
}

fn normalized_range(value: f32, start: f32, end: f32) -> f32 {
    let width = (end - start).max(f32::EPSILON);
    ((value - start) / width).clamp(0.0, 1.0)
}

fn snake_width_scale(length: Option<&TailLength>) -> f32 {
    let Some(length) = length else {
        return 1.0;
    };
    1.0 + normalized_range(
        length.current_size,
        WIDTH_GROWTH_START_LENGTH,
        WIDTH_GROWTH_MAX_LENGTH,
    ) * (WIDTH_GROWTH_MAX_SCALE - 1.0)
}

fn visible_tail(
    head: &SnakeHead,
    points: &TailPoints,
    length: Option<&TailLength>,
) -> TailPolyline {
    let length = length.map(|length| length.current_size).unwrap_or(0.0);
    points.polyline(head, length).axis_aligned()
}

fn snake_visual_color(
    owner: Entity,
    has_player: Option<&HasPlayer>,
    players: &Query<&Player>,
) -> SnakePaletteColor {
    has_player
        .and_then(|has_player| players.get(has_player.0).ok())
        .map(snake_color_for_player)
        .unwrap_or_else(|| snake_color_for_fallback(owner.to_bits()))
}

impl TailLayer {
    const ALL: [Self; 1] = [Self::Core];

    fn width(self, tail_width: f32) -> f32 {
        match self {
            Self::Core => tail_width.max(1.25),
        }
    }

    fn color(self, color: SnakePaletteColor) -> Color {
        match self {
            Self::Core => color.tail_core(),
        }
    }

    fn death_flash_color(self) -> Color {
        match self {
            Self::Core => Color::linear_rgba(7.0, 7.0, 7.0, 1.0),
        }
    }

    fn z(self) -> f32 {
        match self {
            Self::Core => SNAKE_TAIL_Z,
        }
    }

    fn is_glow(self) -> bool {
        !matches!(self, Self::Core)
    }

    fn segment_center_and_length(
        self,
        start: Vec2,
        end: Vec2,
        index: usize,
        head_length: f32,
    ) -> Option<(Vec2, f32)> {
        let delta = end - start;
        let length = delta.length();
        if length <= f32::EPSILON {
            return None;
        }
        if !self.is_glow() || index != 0 {
            return Some(((start + end) * 0.5, length));
        }

        let direction = delta / length;
        let clear_from_head = (head_length * 0.58).min(length);
        let trimmed_end = end - direction * clear_from_head;
        let trimmed_length = trimmed_end.distance(start);
        (trimmed_length > f32::EPSILON).then_some(((start + trimmed_end) * 0.5, trimmed_length))
    }
}

fn spawn_snake_death_animations(
    mut commands: Commands,
    config: Res<GameConfig>,
    mut deaths: MessageReader<ConfirmedDeath>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    tails: Query<(
        Entity,
        &SnakeHead,
        &TailPoints,
        Option<&TailLength>,
        Option<&HasPlayer>,
    )>,
) {
    if !config.render.use_assets {
        for _ in deaths.read() {}
        return;
    }

    let tail_width = config.render.tail_width.max(1.0);
    for death in deaths.read() {
        let live_tail = tails
            .get(death.message.killed_snake)
            .ok()
            .map(|(_, head, tail, length, _)| visible_tail(head, tail, length));
        let tail = if death.local_player {
            death.tail.clone().or(live_tail)
        } else {
            live_tail.or_else(|| death.tail.clone())
        };
        let Some(tail) = tail.as_ref() else {
            if let Some(position) = death.position {
                spawn_death_circle(
                    &mut commands,
                    &mut mesh_assets,
                    &mut materials,
                    position,
                    config.render.head_size.max(3.0) * 0.7,
                    SNAKE_DEATH_Z + 0.2,
                );
            }
            continue;
        };

        for (start, end) in tail.pairs_front_to_back() {
            let delta = end.0 - start.0;
            let length = delta.length();
            if length <= f32::EPSILON {
                continue;
            }
            let rotation = Quat::from_rotation_z(delta.y.atan2(delta.x));
            for layer in TailLayer::ALL {
                let layer_width = layer.width(tail_width);
                let death_width = death_flash_width(layer_width);
                let center = (start.0 + end.0) * 0.5;
                let shape = TailMeshShape::Capsule {
                    length: length.max(death_width),
                    width: death_width,
                };
                commands.spawn((
                    SnakeDeathVisual { elapsed: 0.0 },
                    Mesh2d(mesh_assets.add(shape.mesh())),
                    MeshMaterial2d(materials.add(blended_material(layer.death_flash_color()))),
                    Transform::from_translation(center.extend(SNAKE_DEATH_Z))
                        .with_rotation(rotation),
                ));
            }
        }

        spawn_death_circle(
            &mut commands,
            &mut mesh_assets,
            &mut materials,
            tail.front().0,
            config.render.head_size.max(3.0) * 0.7,
            SNAKE_DEATH_Z + 0.2,
        );

        let joint_count = tail.0.len().saturating_sub(1);
        for (point, _) in tail.0.iter().skip(1).take(joint_count.saturating_sub(1)) {
            for layer in TailLayer::ALL {
                let radius = death_flash_width(layer.width(tail_width)) * 0.5;
                let shape = TailMeshShape::Circle { radius };
                commands.spawn((
                    SnakeDeathVisual { elapsed: 0.0 },
                    Mesh2d(mesh_assets.add(shape.mesh())),
                    MeshMaterial2d(materials.add(blended_material(layer.death_flash_color()))),
                    Transform::from_translation((*point).extend(SNAKE_DEATH_Z)),
                ));
            }
        }
    }
}

fn death_flash_width(layer_width: f32) -> f32 {
    (layer_width * 2.4).max(layer_width + 2.0).min(7.0)
}

fn update_snake_death_animations(
    mut commands: Commands,
    time: Res<Time>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut visuals: Query<(
        Entity,
        &mut SnakeDeathVisual,
        &Mesh2d,
        &MeshMaterial2d<ColorMaterial>,
    )>,
) {
    for (entity, mut visual, mesh, material) in &mut visuals {
        visual.elapsed += time.delta_secs();
        if visual.elapsed >= SNAKE_DEATH_ANIMATION_SECONDS {
            mesh_assets.remove(mesh.0.id());
            materials.remove(material.0.id());
            commands.entity(entity).despawn();
            continue;
        }

        let fade = (1.0 - visual.elapsed / SNAKE_DEATH_ANIMATION_SECONDS).clamp(0.0, 1.0);
        let color = Color::linear_rgba(7.0, 7.0, 7.0, fade);
        set_material_color(&mut materials, material, color);
    }
}

fn spawn_death_circle(
    commands: &mut Commands,
    mesh_assets: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    position: Vec2,
    radius: f32,
    z: f32,
) {
    commands.spawn((
        SnakeDeathVisual { elapsed: 0.0 },
        Mesh2d(mesh_assets.add(TailMeshShape::Circle { radius }.mesh())),
        MeshMaterial2d(materials.add(blended_material(TailLayer::Core.death_flash_color()))),
        Transform::from_translation(position.extend(z)),
    ));
}

impl TailMeshShape {
    fn mesh(self) -> Mesh {
        match self {
            Self::Capsule { length, width } => capsule_mesh(length, width),
            Self::Circle { radius } => circle_mesh(radius),
        }
    }
}

fn blended_material(color: Color) -> ColorMaterial {
    ColorMaterial {
        color,
        alpha_mode: alpha_mode_for_color(color),
        ..default()
    }
}

fn set_material_color(
    materials: &mut Assets<ColorMaterial>,
    handle: &MeshMaterial2d<ColorMaterial>,
    color: Color,
) {
    if let Some(material) = materials.get_mut(&handle.0) {
        material.color = color;
        material.alpha_mode = alpha_mode_for_color(color);
    }
}

fn alpha_mode_for_color(color: Color) -> AlphaMode2d {
    if color.alpha() >= 0.99 {
        AlphaMode2d::Opaque
    } else {
        AlphaMode2d::Blend
    }
}

fn capsule_mesh(length: f32, width: f32) -> Mesh {
    let radius = (width * 0.5).max(0.25);
    let length = length.max(radius * 2.0);
    let half_body = (length * 0.5 - radius).max(0.0);
    if half_body <= f32::EPSILON {
        return circle_mesh(radius);
    }

    let mut perimeter = Vec::with_capacity((MESH_CURVE_SEGMENTS as usize + 1) * 2);
    for step in 0..=MESH_CURVE_SEGMENTS {
        let t = step as f32 / MESH_CURVE_SEGMENTS as f32;
        let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * t;
        perimeter.push(Vec2::new(
            half_body + angle.cos() * radius,
            angle.sin() * radius,
        ));
    }
    for step in 0..=MESH_CURVE_SEGMENTS {
        let t = step as f32 / MESH_CURVE_SEGMENTS as f32;
        let angle = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * t;
        perimeter.push(Vec2::new(
            -half_body + angle.cos() * radius,
            angle.sin() * radius,
        ));
    }
    fan_mesh(perimeter)
}

fn circle_mesh(radius: f32) -> Mesh {
    let radius = radius.max(0.25);
    let mut perimeter = Vec::with_capacity(MESH_CURVE_SEGMENTS as usize * 2);
    let segments = MESH_CURVE_SEGMENTS * 2;
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        let angle = std::f32::consts::TAU * t;
        perimeter.push(Vec2::new(angle.cos() * radius, angle.sin() * radius));
    }
    fan_mesh(perimeter)
}

fn fan_mesh(perimeter: Vec<Vec2>) -> Mesh {
    let mut positions = Vec::with_capacity(perimeter.len() + 1);
    let mut normals = Vec::with_capacity(perimeter.len() + 1);
    let mut uvs = Vec::with_capacity(perimeter.len() + 1);
    positions.push([0.0, 0.0, 0.0]);
    normals.push([0.0, 0.0, 1.0]);
    uvs.push([0.5, 0.5]);

    let uv_extent = perimeter
        .iter()
        .fold(1.0_f32, |extent, point| extent.max(point.length() * 2.0));
    for point in &perimeter {
        positions.push([point.x, point.y, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([0.5 + point.x / uv_extent, 0.5 + point.y / uv_extent]);
    }

    let mut indices = Vec::with_capacity(perimeter.len() * 3);
    for index in 1..=perimeter.len() as u32 {
        let next = if index == perimeter.len() as u32 {
            1
        } else {
            index + 1
        };
        indices.extend_from_slice(&[0, index, next]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn direction_rotation(direction: Direction) -> Quat {
    let delta = direction.delta();
    Quat::from_rotation_z(delta.y.atan2(delta.x))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn tail_is_axis_aligned(tail: &TailPolyline) -> bool {
        tail.pairs_front_to_back()
            .all(|(start, end)| segment_is_axis_aligned(start.0, end.0))
    }

    fn segment_is_axis_aligned(start: Vec2, end: Vec2) -> bool {
        (start.x - end.x).abs() <= f32::EPSILON * 8.0
            || (start.y - end.y).abs() <= f32::EPSILON * 8.0
    }

    #[test]
    fn death_animation_spawns_position_fallback_without_tail_snapshot() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<ColorMaterial>>();
        app.add_message::<ConfirmedDeath>();
        app.add_systems(Update, spawn_snake_death_animations);

        app.world_mut()
            .resource_mut::<Messages<ConfirmedDeath>>()
            .write(ConfirmedDeath {
                message: PlayerDeath {
                    killer_player: Entity::from_bits(1),
                    killed_player: Entity::from_bits(2),
                    killer_snake: Entity::from_bits(3),
                    killed_snake: Entity::from_bits(4),
                    killer_name: "killer".to_string(),
                    killed_name: "killed".to_string(),
                    room: RoomId(1),
                    reason: DeathReason::Collision,
                    position: Vec2::new(12.0, 34.0),
                    stats: PlayerDeathStats::default(),
                },
                local_player: true,
                position: Some(Vec2::new(12.0, 34.0)),
                tail: None,
            });

        app.update();

        let mut query = app.world_mut().query::<&SnakeDeathVisual>();
        assert_eq!(query.iter(app.world()).count(), 1);
    }

    #[test]
    fn visible_tail_repairs_transient_diagonal_segments() {
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::new(10.0, 10.0), Direction::Right),
            (Vec2::ZERO, Direction::Right),
        ]));

        let visible = tail.axis_aligned();

        assert!(tail_is_axis_aligned(&visible));
        assert_eq!(
            visible.0,
            VecDeque::from([
                (Vec2::new(10.0, 10.0), Direction::Right),
                (Vec2::new(10.0, 0.0), Direction::Up),
                (Vec2::ZERO, Direction::Right),
            ])
        );
    }

    #[test]
    fn visible_tail_clips_repaired_path_to_length() {
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::new(10.0, 10.0), Direction::Right),
            (Vec2::ZERO, Direction::Right),
        ]));

        let visible = tail.axis_aligned().clipped_to_length(10.0);

        assert!(tail_is_axis_aligned(&visible));
        assert_eq!(visible.total_length(), 10.0);
        assert_eq!(
            visible.0,
            VecDeque::from([
                (Vec2::new(10.0, 10.0), Direction::Right),
                (Vec2::new(10.0, 0.0), Direction::Up),
            ])
        );
    }

    #[test]
    fn accelerating_snake_head_glow_starts_after_minimum_speed_and_expands() {
        let config = GameConfig::default();
        let head_diameter = 4.0;
        let idle = head_glow_visual(
            head_diameter,
            Some(&Speed(config.movement.min_speed)),
            Some(&Acceleration(config.movement.base_acceleration)),
            &config,
        );
        let accelerating_at_minimum = head_glow_visual(
            head_diameter,
            Some(&Speed(config.movement.min_speed)),
            Some(&Acceleration(0.01)),
            &config,
        );
        let accelerating_mid = head_glow_visual(
            head_diameter,
            Some(&Speed(
                (config.movement.min_speed + config.movement.max_speed) * 0.5,
            )),
            Some(&Acceleration(0.01)),
            &config,
        );
        let accelerating_fast = head_glow_visual(
            head_diameter,
            Some(&Speed(config.movement.max_speed)),
            Some(&Acceleration(0.01)),
            &config,
        );

        assert_eq!(idle.alpha, 0.0);
        assert_eq!(accelerating_at_minimum.alpha, 0.0);
        assert!(accelerating_mid.diameter > idle.diameter);
        assert!(accelerating_mid.alpha > idle.alpha);
        assert!(accelerating_fast.diameter > accelerating_mid.diameter);
        assert!(accelerating_fast.alpha > accelerating_mid.alpha);
    }

    #[test]
    fn snake_width_scale_reaches_configured_large_length_width() {
        assert_eq!(snake_width_scale(None), 1.0);
        assert_eq!(
            snake_width_scale(Some(&TailLength {
                current_size: WIDTH_GROWTH_START_LENGTH,
                target_size: WIDTH_GROWTH_START_LENGTH,
            })),
            1.0
        );
        assert_eq!(
            snake_width_scale(Some(&TailLength {
                current_size: WIDTH_GROWTH_MAX_LENGTH,
                target_size: WIDTH_GROWTH_MAX_LENGTH,
            })),
            WIDTH_GROWTH_MAX_SCALE
        );
        assert_eq!(
            snake_width_scale(Some(&TailLength {
                current_size: (WIDTH_GROWTH_START_LENGTH + WIDTH_GROWTH_MAX_LENGTH) * 0.5,
                target_size: WIDTH_GROWTH_MAX_LENGTH,
            })),
            1.0 + (WIDTH_GROWTH_MAX_SCALE - 1.0) * 0.5
        );
    }
}
