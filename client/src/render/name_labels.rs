use std::collections::HashSet;

use bevy::ecs::query::Or;
use bevy::prelude::*;
use bevy::sprite::Text2d;
use bevy::transform::TransformSystems;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{ConfirmedHistory, Interpolated, Predicted, Replicated};
use shared::network::protocol::prelude::{HasPlayer, Player, PlayerStatus, SnakeHead, TailPoints};

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
            (sync_name_label_roots, update_name_labels)
                .chain()
                .after(FrameInterpolationSystems::Interpolate)
                .before(TransformSystems::Propagate),
        );
    }
}

fn sync_name_label_roots(
    mut commands: Commands,
    mut snakes: Query<
        (
            Entity,
            &SnakeHead,
            Option<&mut Transform>,
            Option<&GlobalTransform>,
            Option<&Visibility>,
            Option<&InheritedVisibility>,
            Option<&ViewVisibility>,
        ),
        (
            With<TailPoints>,
            Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
        ),
    >,
) {
    for (
        snake,
        head,
        transform,
        global_transform,
        visibility,
        inherited_visibility,
        view_visibility,
    ) in &mut snakes
    {
        let translation = head.position.extend(0.0);
        if let Some(mut transform) = transform {
            transform.translation = translation;
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::ONE;
        } else {
            commands
                .entity(snake)
                .insert(Transform::from_translation(translation));
        }

        if global_transform.is_none() {
            commands.entity(snake).insert(GlobalTransform::default());
        }

        if visibility != Some(&Visibility::Inherited) {
            commands.entity(snake).insert(Visibility::Inherited);
        }
        if inherited_visibility.is_none() {
            commands
                .entity(snake)
                .insert(InheritedVisibility::default());
        }
        if view_visibility.is_none() {
            commands.entity(snake).insert(ViewVisibility::default());
        }
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
            Option<&ConfirmedHistory<HasPlayer>>,
            Has<Predicted>,
            Has<Interpolated>,
            Has<Replicated>,
        ),
        (
            With<TailPoints>,
            Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
        ),
    >,
    mut labels: Query<(
        Entity,
        &mut NameLabel,
        Option<&ChildOf>,
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
            let snake = visible_snake_for_player(player_entity, player, &tails)?;
            Some((
                player_entity,
                snake,
                player.name.clone(),
                snake_color_for_player(player).label(),
            ))
        })
        .collect::<Vec<_>>();

    let mut existing = HashSet::new();
    for (
        label_entity,
        mut label,
        parent,
        mut text,
        mut text_color,
        mut transform,
        mut visibility,
    ) in &mut labels
    {
        if let Some((_, snake, name, color)) = wanted
            .iter()
            .find(|(player_entity, _, _, _)| *player_entity == label.player)
        {
            if label.snake != *snake || parent.map(ChildOf::parent) != Some(*snake) {
                commands.entity(label_entity).despawn();
                continue;
            }

            label.snake = *snake;
            existing.insert((label.player, *snake, label.layer));
            if text.0 != *name {
                text.0 = name.clone();
            }
            *text_color = TextColor(label_color(label.layer, *color));
            transform.translation = label_local_translation(label.layer);
            *visibility = Visibility::Inherited;
        } else {
            commands.entity(label_entity).despawn();
        }
    }

    for (player_entity, snake, name, color) in wanted {
        for layer in NameLabelLayer::ALL {
            if existing.contains(&(player_entity, snake, layer)) {
                continue;
            }
            commands.entity(snake).with_children(|parent| {
                parent.spawn((
                    NameLabel {
                        player: player_entity,
                        snake,
                        layer,
                    },
                    Text2d::new(name.clone()),
                    TextFont::from_font_size(LABEL_FONT_SIZE),
                    TextColor(label_color(layer, color)),
                    TextLayout::new_with_justify(Justify::Left),
                    Transform::from_translation(label_local_translation(layer)),
                    GlobalTransform::default(),
                    Visibility::Inherited,
                    InheritedVisibility::default(),
                    ViewVisibility::default(),
                ));
            });
        }
    }
}

fn visible_snake_for_player(
    player_entity: Entity,
    player: &Player,
    tails: &Query<
        (
            Entity,
            &SnakeHead,
            Option<&HasPlayer>,
            Option<&ConfirmedHistory<HasPlayer>>,
            Has<Predicted>,
            Has<Interpolated>,
            Has<Replicated>,
        ),
        (
            With<TailPoints>,
            Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
        ),
    >,
) -> Option<Entity> {
    tails
        .iter()
        .filter(|(snake_entity, _, owner, owner_history, _, _, _)| {
            snake_owner(*owner, *owner_history) == Some(player_entity)
                || player.snake == Some(*snake_entity)
        })
        .max_by_key(|(_, _, _, _, predicted, interpolated, replicated)| {
            visible_snake_priority(*predicted, *interpolated, *replicated)
        })
        .map(|(snake_entity, _, _, _, _, _, _)| snake_entity)
}

fn snake_owner(
    owner: Option<&HasPlayer>,
    owner_history: Option<&ConfirmedHistory<HasPlayer>>,
) -> Option<Entity> {
    owner.map(|owner| owner.0).or_else(|| {
        owner_history.and_then(|history| history.newest_present().map(|(_, owner)| owner.0))
    })
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

fn label_local_translation(layer: NameLabelLayer) -> Vec3 {
    let offset = match layer {
        NameLabelLayer::Shadow => LABEL_OFFSET + LABEL_SHADOW_OFFSET,
        NameLabelLayer::Text => LABEL_OFFSET,
    };
    let z = match layer {
        NameLabelLayer::Shadow => LABEL_SHADOW_Z,
        NameLabelLayer::Text => LABEL_Z,
    };
    Vec3::new(offset.x, offset.y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightyear::prelude::{ConfirmedState, Tick};

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

    #[test]
    fn snake_owner_falls_back_to_confirmed_history() {
        let player = Entity::from_bits(42);
        let mut history = ConfirmedHistory::default();
        history.insert(Tick(10), ConfirmedState::Confirmed(HasPlayer(player)));

        assert_eq!(snake_owner(None, Some(&history)), Some(player));
    }

    #[test]
    fn update_name_labels_spawns_visible_text_children() {
        let mut app = App::new();
        app.add_systems(Update, (sync_name_label_roots, update_name_labels).chain());

        let snake = app
            .world_mut()
            .spawn((
                SnakeHead {
                    position: Vec2::new(10.0, 20.0),
                    ..default()
                },
                TailPoints::empty(),
            ))
            .id();
        let player = app
            .world_mut()
            .spawn((
                Player {
                    id: lightyear::prelude::PeerId::Netcode(1),
                    name: "Alice".to_string(),
                    snake: Some(snake),
                },
                PlayerStatus::Alive,
            ))
            .id();
        app.world_mut().entity_mut(snake).insert(HasPlayer(player));

        app.update();

        assert_eq!(
            app.world().get::<Visibility>(snake),
            Some(&Visibility::Inherited)
        );
        assert!(app.world().get::<InheritedVisibility>(snake).is_some());
        assert!(app.world().get::<ViewVisibility>(snake).is_some());

        let mut labels = app
            .world_mut()
            .query::<(&NameLabel, &Text2d, &ChildOf, &Visibility)>();
        let labels = labels.iter(app.world()).collect::<Vec<_>>();

        assert_eq!(labels.len(), NameLabelLayer::ALL.len());
        for (label, text, parent, visibility) in labels {
            assert_eq!(label.player, player);
            assert_eq!(label.snake, snake);
            assert_eq!(text.0, "Alice");
            assert_eq!(parent.parent(), snake);
            assert_eq!(*visibility, Visibility::Inherited);
        }
    }
}
