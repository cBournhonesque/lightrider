use std::collections::HashSet;

use bevy::ecs::query::Or;
use bevy::prelude::*;
use bevy::sprite::Text2d;
use bevy::transform::TransformSystems;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Interpolated, Predicted, Replicated};
use shared::network::protocol::prelude::{HasPlayer, Player, PlayerStatus, SnakeHead};

use crate::render::colors::snake_color_for_player;

const LABEL_OFFSET: Vec2 = Vec2::new(14.0, 12.0);
const LABEL_SHADOW_OFFSET: Vec2 = Vec2::new(1.0, -1.0);
const LABEL_Z: f32 = 20.0;
const LABEL_SHADOW_Z: f32 = LABEL_Z - 0.01;
const LABEL_FONT_SIZE: f32 = 10.0;

pub(crate) struct NameLabelRenderPlugin;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NameLabel {
    player: Entity,
    snake: Entity,
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
        (
            Entity,
            &SnakeHead,
            Option<&HasPlayer>,
            Has<Predicted>,
            Has<Interpolated>,
            Has<Replicated>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
    mut labels: Query<(
        Entity,
        &mut NameLabel,
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
            let (snake, head) = visible_snake_for_player(player_entity, player, &tails)?;
            Some((
                player_entity,
                snake,
                player.name.clone(),
                head.position,
                snake_color_for_player(player).label(),
            ))
        })
        .collect::<Vec<_>>();

    let mut existing = HashSet::new();
    for (label_entity, mut label, mut text, mut text_color, mut transform, mut visibility) in
        &mut labels
    {
        if let Some((_, snake, name, head, color)) = wanted
            .iter()
            .find(|(player_entity, _, _, _, _)| *player_entity == label.player)
        {
            label.snake = *snake;
            existing.insert((label.player, *snake, label.layer));
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

    for (player_entity, snake, name, head, color) in wanted {
        for layer in NameLabelLayer::ALL {
            if existing.contains(&(player_entity, snake, layer)) {
                continue;
            }
            commands.spawn((
                NameLabel {
                    player: player_entity,
                    snake,
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

fn visible_snake_for_player<'a>(
    player_entity: Entity,
    player: &Player,
    tails: &'a Query<
        (
            Entity,
            &SnakeHead,
            Option<&HasPlayer>,
            Has<Predicted>,
            Has<Interpolated>,
            Has<Replicated>,
        ),
        Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
    >,
) -> Option<(Entity, &'a SnakeHead)> {
    tails
        .iter()
        .filter(|(snake_entity, _, owner, _, _, _)| {
            owner.is_some_and(|owner| owner.0 == player_entity)
                || player.snake == Some(*snake_entity)
        })
        .max_by_key(|(_, _, _, predicted, interpolated, replicated)| {
            visible_snake_priority(*predicted, *interpolated, *replicated)
        })
        .map(|(snake_entity, head, _, _, _, _)| (snake_entity, head))
}

fn visible_snake_priority(predicted: bool, interpolated: bool, replicated: bool) -> u8 {
    if predicted {
        3
    } else if interpolated {
        2
    } else if !replicated {
        1
    } else {
        0
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_snake_priority_prefers_rendered_entities() {
        assert!(
            visible_snake_priority(true, false, false) > visible_snake_priority(false, true, false)
        );
        assert!(
            visible_snake_priority(false, true, false)
                > visible_snake_priority(false, false, false)
        );
        assert!(
            visible_snake_priority(false, false, false)
                > visible_snake_priority(false, false, true)
        );
    }
}
