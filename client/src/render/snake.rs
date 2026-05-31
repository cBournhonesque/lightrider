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
const SNAKE_TAIL_Z: f32 = 10.0;
const SNAKE_DEATH_ANIMATION_SECONDS: f32 = 0.54;
const SNAKE_DEATH_FLASH_SECONDS: f32 = 0.1;
const MESH_CURVE_SEGMENTS: u32 = 14;

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
enum TailMeshShape {
    Capsule { length: f32, width: f32 },
    Circle { radius: f32 },
}

#[derive(Component, Clone, Copy, Debug)]
struct SnakeDeathVisual {
    elapsed: f32,
    layer: TailLayer,
    color: SnakePaletteColor,
    width: f32,
    target: Vec2,
    kind: SnakeDeathVisualKind,
}

#[derive(Clone, Copy, Debug)]
enum SnakeDeathVisualKind {
    Segment { start: Vec2, end: Vec2 },
    Joint { position: Vec2 },
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
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    players: Query<&Player>,
    tails: Query<
        (Entity, &TailPoints, Option<&HasPlayer>),
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
        (Entity, &TailPoints, Option<&HasPlayer>),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) -> (Vec<DesiredSnakeSpriteVisual>, Vec<DesiredSnakeMeshVisual>) {
    let tail_width = config.render.tail_width.max(1.0);
    let head_size = config.render.head_size.max(tail_width * 1.8);
    let head_length = (head_size * 2.4).max(tail_width * 8.0);
    let head_width = (head_size * 0.55).max(tail_width * 3.0);
    let mut sprite_desired = Vec::new();
    let mut mesh_desired = Vec::new();

    for (owner, points, player) in tails.iter() {
        let color = snake_visual_color(owner, player, players);
        let head = points.front().0;
        sprite_desired.push(DesiredSnakeSpriteVisual {
            key: SnakeVisualKey {
                owner,
                part: SnakeVisualPart::Head,
            },
            transform: Transform::from_translation(head.extend(SNAKE_HEAD_Z))
                .with_rotation(direction_rotation(points.front().1)),
            sprite: sheet.sprite(
                PowerlineFrame::HeadDot,
                Vec2::new(head_length, head_width),
                color.head(),
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
                    layer.segment_center_and_length(start.0, end.0, index, head_length)
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
    players: Query<&Player>,
    tails: Query<(Entity, &TailPoints, Option<&HasPlayer>)>,
) {
    if !config.render.use_assets {
        for _ in deaths.read() {}
        return;
    }

    let tail_width = config.render.tail_width.max(1.0);
    for death in deaths.read() {
        let live_tail = tails.get(death.message.killed_snake).ok();
        let tail = live_tail.map(|(_, tail, _)| tail).or(death.tail.as_ref());
        let Some(tail) = tail else {
            continue;
        };
        let color = live_tail
            .map(|(snake, _, has_player)| snake_visual_color(snake, has_player, &players))
            .or_else(|| {
                players
                    .get(death.message.killed_player)
                    .ok()
                    .map(snake_color_for_player)
            })
            .unwrap_or_else(|| snake_color_for_fallback(death.message.killed_snake.to_bits()));
        let target = tail_midpoint(tail).unwrap_or_else(|| death.position.unwrap_or(Vec2::ZERO));

        for (start, end) in tail.pairs_front_to_back() {
            let delta = end.0 - start.0;
            let length = delta.length();
            if length <= f32::EPSILON {
                continue;
            }
            let rotation = Quat::from_rotation_z(delta.y.atan2(delta.x));
            for layer in TailLayer::ALL {
                let layer_width = layer.width(tail_width);
                let center = (start.0 + end.0) * 0.5;
                let shape = TailMeshShape::Capsule {
                    length: length.max(layer_width),
                    width: layer_width,
                };
                commands.spawn((
                    SnakeDeathVisual {
                        elapsed: 0.0,
                        layer,
                        color,
                        width: layer_width,
                        target,
                        kind: SnakeDeathVisualKind::Segment {
                            start: start.0,
                            end: end.0,
                        },
                    },
                    Mesh2d(mesh_assets.add(shape.mesh())),
                    MeshMaterial2d(materials.add(blended_material(layer.death_flash_color()))),
                    Transform::from_translation(center.extend(layer.z() + 0.2))
                        .with_rotation(rotation),
                ));
            }
        }

        let joint_count = tail.0.len().saturating_sub(1);
        for (point, _) in tail.0.iter().skip(1).take(joint_count.saturating_sub(1)) {
            for layer in TailLayer::ALL {
                let radius = layer.width(tail_width) * 0.5;
                let shape = TailMeshShape::Circle { radius };
                commands.spawn((
                    SnakeDeathVisual {
                        elapsed: 0.0,
                        layer,
                        color,
                        width: radius * 2.0,
                        target,
                        kind: SnakeDeathVisualKind::Joint { position: *point },
                    },
                    Mesh2d(mesh_assets.add(shape.mesh())),
                    MeshMaterial2d(materials.add(blended_material(layer.death_flash_color()))),
                    Transform::from_translation((*point).extend(layer.z() + 0.2)),
                ));
            }
        }
    }
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
        &mut Transform,
    )>,
) {
    for (entity, mut visual, mesh, material, mut transform) in &mut visuals {
        visual.elapsed += time.delta_secs();
        if visual.elapsed >= SNAKE_DEATH_ANIMATION_SECONDS {
            mesh_assets.remove(mesh.0.id());
            materials.remove(material.0.id());
            commands.entity(entity).despawn();
            continue;
        }

        let collapse_t = ((visual.elapsed - SNAKE_DEATH_FLASH_SECONDS)
            / (SNAKE_DEATH_ANIMATION_SECONDS - SNAKE_DEATH_FLASH_SECONDS))
            .clamp(0.0, 1.0);
        let eased = smoothstep(collapse_t);
        let fade = (1.0 - collapse_t).clamp(0.0, 1.0);
        let color = if visual.elapsed <= SNAKE_DEATH_FLASH_SECONDS {
            visual.layer.death_flash_color()
        } else {
            faded_color(visual.layer.color(visual.color), fade)
        };
        set_material_color(&mut materials, material, color);

        match visual.kind {
            SnakeDeathVisualKind::Segment { start, end } => {
                let start = start.lerp(visual.target, eased);
                let end = end.lerp(visual.target, eased);
                let delta = end - start;
                let length = delta.length();
                if length <= 0.1 {
                    if let Some(mesh_asset) = mesh_assets.get_mut(&mesh.0) {
                        *mesh_asset = TailMeshShape::Circle {
                            radius: visual.width * 0.2 * fade.max(0.1),
                        }
                        .mesh();
                    }
                    transform.translation = visual.target.extend(visual.layer.z() + 0.2);
                    transform.rotation = Quat::IDENTITY;
                    continue;
                }

                let width = visual.width * (0.25 + 0.75 * fade);
                let shape = TailMeshShape::Capsule {
                    length: length.max(width),
                    width,
                };
                if let Some(mesh_asset) = mesh_assets.get_mut(&mesh.0) {
                    *mesh_asset = shape.mesh();
                }
                transform.translation = ((start + end) * 0.5).extend(visual.layer.z() + 0.2);
                transform.rotation = Quat::from_rotation_z(delta.y.atan2(delta.x));
            }
            SnakeDeathVisualKind::Joint { position } => {
                let position = position.lerp(visual.target, eased);
                let radius = visual.width * 0.5 * (0.2 + 0.8 * fade);
                if let Some(mesh_asset) = mesh_assets.get_mut(&mesh.0) {
                    *mesh_asset = TailMeshShape::Circle { radius }.mesh();
                }
                transform.translation = position.extend(visual.layer.z() + 0.2);
            }
        }
    }
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
        alpha_mode: AlphaMode2d::Blend,
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
        material.alpha_mode = AlphaMode2d::Blend;
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

fn tail_midpoint(tail: &TailPoints) -> Option<Vec2> {
    let total = tail.total_length();
    if total <= f32::EPSILON {
        return tail.0.front().map(|(point, _)| *point);
    }
    tail_position_at_distance(tail, total * 0.5)
}

fn tail_position_at_distance(tail: &TailPoints, distance: f32) -> Option<Vec2> {
    let mut remaining = distance.max(0.0);
    for (start, end) in tail.pairs_front_to_back() {
        let delta = end.0 - start.0;
        let length = delta.length();
        if length <= f32::EPSILON {
            continue;
        }
        if remaining <= length {
            return Some(start.0 + delta / length * remaining);
        }
        remaining -= length;
    }
    tail.0.back().map(|(point, _)| *point)
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn faded_color(color: Color, alpha_scale: f32) -> Color {
    let mut color = color;
    color.set_alpha(color.alpha() * alpha_scale.clamp(0.0, 1.0));
    color
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
