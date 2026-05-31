use std::collections::HashSet;

use bevy::prelude::*;
use bevy::sprite::Text2d;
use bevy::transform::TransformSystems;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use shared::network::protocol::prelude::{Player, TailPoints};

use crate::render::colors::snake_color_for_player;

const LABEL_OFFSET: Vec2 = Vec2::new(14.0, 13.0);
const LABEL_Z: f32 = 20.0;
const LABEL_FONT_SIZE: f32 = 9.0;

pub(crate) struct NameLabelRenderPlugin;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NameLabel {
    player: Entity,
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
    players: Query<(Entity, &Player)>,
    tails: Query<&TailPoints>,
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
        .filter_map(|(player_entity, player)| {
            let tail = player.snake.and_then(|snake| tails.get(snake).ok())?;
            Some((
                player_entity,
                player.name.clone(),
                tail.front().0,
                snake_color_for_player(player).label(),
            ))
        })
        .collect::<Vec<_>>();

    let mut existing_players = HashSet::new();
    for (label_entity, label, mut text, mut text_color, mut transform, mut visibility) in
        &mut labels
    {
        if let Some((_, name, head, color)) = wanted
            .iter()
            .find(|(player_entity, _, _, _)| *player_entity == label.player)
        {
            existing_players.insert(label.player);
            if text.0 != *name {
                text.0 = name.clone();
            }
            *text_color = TextColor(*color);
            transform.translation = label_translation(*head);
            *visibility = Visibility::Inherited;
        } else {
            commands.entity(label_entity).despawn();
        }
    }

    for (player_entity, name, head, color) in wanted {
        if existing_players.contains(&player_entity) {
            continue;
        }
        commands.spawn((
            NameLabel {
                player: player_entity,
            },
            Text2d::new(name),
            TextFont::from_font_size(LABEL_FONT_SIZE),
            TextColor(color),
            TextLayout::new_with_justify(Justify::Left),
            Transform::from_translation(label_translation(head)),
        ));
    }
}

fn label_translation(head: Vec2) -> Vec3 {
    Vec3::new(head.x + LABEL_OFFSET.x, head.y + LABEL_OFFSET.y, LABEL_Z)
}
