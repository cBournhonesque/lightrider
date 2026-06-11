use std::collections::HashSet;

use bevy::ecs::query::Or;
use bevy::prelude::*;
use bevy::sprite::Text2d;
use bevy::transform::TransformSystems;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Interpolated, Predicted, Replicated};
use shared::network::protocol::prelude::{HasPlayer, Player, PlayerStatus, TailPoints};

use crate::render::colors::snake_color_for_player;

const LABEL_OFFSET: Vec2 = Vec2::new(18.0, 16.0);
const LABEL_SHADOW_OFFSET: Vec2 = Vec2::new(1.25, -1.25);
const LABEL_Z: f32 = 20.0;
const LABEL_SHADOW_Z: f32 = LABEL_Z - 0.01;
const LABEL_FONT_SIZE: f32 = 14.0;

pub(crate) struct NameLabelRenderPlugin;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NameLabel {
    player: Entity,
    layer: NameLabelLayer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum NameLabelLayer {
    Shadow,
    Text,
}

impl NameLabelLayer {
    const ALL: [Self; 2] = [Self::Shadow, Self::Text];
}

impl Plugin for NameLabelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            update_name_labels
                .after(FrameInterpolationSystems::Interpolate)
                .before(TransformSystems::Propagate),
        );
    }
}

fn update_name_labels(
    mut commands: Commands,
    players: Query<(Entity, &Player, &PlayerStatus)>,
    tails: Query<
        (Entity, &TailPoints, Option<&HasPlayer>),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    mut labels: Query<(
        Entity,
        &NameLabel,
        &mut Text2d,
        &mut TextColor,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    let wanted = players
        .iter()
        .filter(|(_, _, status)| **status == PlayerStatus::Alive)
        .filter_map(|(player_entity, player, _)| {
            let tail = tail_for_player(player_entity, player, &tails)?;
            Some((
                player_entity,
                player.name.clone(),
                tail.front().0,
                snake_color_for_player(player).label(),
            ))
        })
        .collect::<Vec<_>>();

    let mut existing = HashSet::new();
    for (label_entity, label, mut text, mut text_color, mut transform, mut visibility) in
        &mut labels
    {
        if let Some((_, name, head, color)) = wanted
            .iter()
            .find(|(player_entity, _, _, _)| *player_entity == label.player)
        {
            existing.insert((label.player, label.layer));
            if text.0 != *name {
                text.0 = name.clone();
            }
            *text_color = TextColor(label_color(label.layer, *color));
            transform.translation = label_translation(*head, label.layer);
            *visibility = Visibility::Inherited;
        } else {
            commands.entity(label_entity).despawn();
        }
    }

    for (player_entity, name, head, color) in wanted {
        for layer in NameLabelLayer::ALL {
            if existing.contains(&(player_entity, layer)) {
                continue;
            }
            commands.spawn((
                NameLabel {
                    player: player_entity,
                    layer,
                },
                Text2d::new(name.clone()),
                TextFont::from_font_size(LABEL_FONT_SIZE),
                TextColor(label_color(layer, color)),
                TextLayout::new_with_justify(Justify::Left),
                Transform::from_translation(label_translation(head, layer)),
            ));
        }
    }
}

fn tail_for_player<'a>(
    player_entity: Entity,
    player: &Player,
    tails: &'a Query<
        (Entity, &TailPoints, Option<&HasPlayer>),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) -> Option<&'a TailPoints> {
    tails.iter().find_map(|(snake_entity, tail, owner)| {
        if owner.is_some_and(|owner| owner.0 == player_entity) || player.snake == Some(snake_entity)
        {
            Some(tail)
        } else {
            None
        }
    })
}

fn label_color(layer: NameLabelLayer, color: Color) -> Color {
    match layer {
        NameLabelLayer::Shadow => Color::srgba(0.0, 0.02, 0.04, 0.88),
        NameLabelLayer::Text => color,
    }
}

fn label_translation(head: Vec2, layer: NameLabelLayer) -> Vec3 {
    let offset = match layer {
        NameLabelLayer::Shadow => LABEL_OFFSET + LABEL_SHADOW_OFFSET,
        NameLabelLayer::Text => LABEL_OFFSET,
    };
    let z = match layer {
        NameLabelLayer::Shadow => LABEL_SHADOW_Z,
        NameLabelLayer::Text => LABEL_Z,
    };
    Vec3::new(head.x + offset.x, head.y + offset.y, z)
}
