use std::collections::HashSet;

use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{ConfirmedHistory, Interpolated, Predicted, Replicated};
use shared::network::protocol::prelude::{HasPlayer, Player, PlayerStatus, SnakeHead, TailPoints};

use crate::camera::CameraSystems;
use crate::render::colors::snake_color_for_player;

const LABEL_OFFSET_PX: Vec2 = Vec2::new(18.0, 10.0);
const LABEL_SHADOW_OFFSET_PX: Vec2 = Vec2::new(1.0, -1.0);
const LABEL_FONT_SIZE_PX: f32 = 13.0;
const LABEL_Z_INDEX: i32 = 42;
const LABEL_SHADOW_Z_INDEX: i32 = LABEL_Z_INDEX - 1;

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

#[derive(Clone)]
struct DesiredNameLabel {
    player: Entity,
    snake: Entity,
    head_viewport_position: Option<Vec2>,
    name: String,
    color: Color,
}

impl Plugin for NameLabelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            update_name_labels
                .after(FrameInterpolationSystems::Interpolate)
                .after(CameraSystems::Follow),
        );
    }
}

fn update_name_labels(
    mut commands: Commands,
    players: Query<(Entity, &Player, Option<&PlayerStatus>)>,
    cameras: Query<(&Camera, &Transform), With<Camera2d>>,
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
        With<TailPoints>,
    >,
    mut labels: Query<(
        Entity,
        &mut NameLabel,
        &mut Text,
        &mut TextColor,
        &mut Node,
        &mut Visibility,
    )>,
) {
    let camera = cameras.single().ok();
    let wanted = players
        .iter()
        .filter(|(_, _, status)| status.is_none_or(|status| *status == PlayerStatus::Alive))
        .filter_map(|(player_entity, player, _)| {
            let (snake, head) = visible_snake_for_player(player_entity, player, &tails)?;
            Some(DesiredNameLabel {
                player: player_entity,
                snake,
                head_viewport_position: head_viewport_position(head, camera),
                name: label_name(player),
                color: snake_color_for_player(player).label(),
            })
        })
        .collect::<Vec<_>>();

    let mut existing = HashSet::new();
    for (label_entity, mut label, mut text, mut text_color, mut node, mut visibility) in &mut labels
    {
        if let Some(desired) = wanted
            .iter()
            .find(|candidate| candidate.player == label.player)
        {
            label.snake = desired.snake;
            existing.insert((label.player, desired.snake, label.layer));
            if text.0 != desired.name {
                text.0 = desired.name.clone();
            }
            *text_color = TextColor(label_color(label.layer, desired.color));
            if let Some(head_viewport_position) = desired.head_viewport_position {
                *node = label_node(head_viewport_position, label.layer);
                *visibility = Visibility::Visible;
            } else {
                *visibility = Visibility::Hidden;
            }
        } else {
            commands.entity(label_entity).despawn();
        }
    }

    for desired in wanted {
        for layer in NameLabelLayer::ALL {
            if existing.contains(&(desired.player, desired.snake, layer)) {
                continue;
            }
            let visibility = if desired.head_viewport_position.is_some() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            commands.spawn((
                NameLabel {
                    player: desired.player,
                    snake: desired.snake,
                    layer,
                },
                label_node(desired.head_viewport_position.unwrap_or(Vec2::ZERO), layer),
                Text::new(desired.name.clone()),
                TextFont::from_font_size(LABEL_FONT_SIZE_PX),
                TextColor(label_color(layer, desired.color)),
                TextLayout::justify(Justify::Left),
                ZIndex(label_z_index(layer)),
                visibility,
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
            Option<&ConfirmedHistory<HasPlayer>>,
            Has<Predicted>,
            Has<Interpolated>,
            Has<Replicated>,
        ),
        With<TailPoints>,
    >,
) -> Option<(Entity, &'a SnakeHead)> {
    tails
        .iter()
        .filter(|(snake_entity, _, owner, owner_history, _, _, _)| {
            snake_owner(*owner, *owner_history) == Some(player_entity)
                || player.snake == Some(*snake_entity)
        })
        .max_by_key(|(_, _, _, _, predicted, interpolated, replicated)| {
            visible_snake_priority(*predicted, *interpolated, *replicated)
        })
        .map(|(snake_entity, head, _, _, _, _, _)| (snake_entity, head))
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

fn label_name(player: &Player) -> String {
    let trimmed = player.name.trim();
    if trimmed.is_empty() {
        format!("Player {}", player.id.to_bits())
    } else {
        trimmed.to_string()
    }
}

fn head_viewport_position(head: &SnakeHead, camera: Option<(&Camera, &Transform)>) -> Option<Vec2> {
    match camera {
        Some((camera, camera_transform)) => {
            let camera_transform = GlobalTransform::from(*camera_transform);
            camera
                .world_to_viewport(&camera_transform, head.position.extend(0.0))
                .ok()
        }
        None => Some(head.position),
    }
}

fn label_node(head_viewport_position: Vec2, layer: NameLabelLayer) -> Node {
    let position = label_screen_position(head_viewport_position, layer);
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(position.x),
        top: Val::Px(position.y),
        ..default()
    }
}

fn label_screen_position(head_viewport_position: Vec2, layer: NameLabelLayer) -> Vec2 {
    let offset = match layer {
        NameLabelLayer::Shadow => LABEL_OFFSET_PX + LABEL_SHADOW_OFFSET_PX,
        NameLabelLayer::Text => LABEL_OFFSET_PX,
    };
    Vec2::new(
        head_viewport_position.x + offset.x,
        head_viewport_position.y - offset.y,
    )
}

fn label_z_index(layer: NameLabelLayer) -> i32 {
    match layer {
        NameLabelLayer::Shadow => LABEL_SHADOW_Z_INDEX,
        NameLabelLayer::Text => LABEL_Z_INDEX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightyear::prelude::{HistoryState, Tick};

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
        history.insert(Tick(10), HistoryState::Updated(HasPlayer(player)));

        assert_eq!(snake_owner(None, Some(&history)), Some(player));
    }

    #[test]
    fn empty_player_name_uses_id_fallback() {
        let player = Player {
            id: lightyear::prelude::PeerId::Netcode(7),
            name: "  ".to_string(),
            snake: None,
        };

        assert_eq!(label_name(&player), "Player 7");
    }

    #[test]
    fn update_name_labels_spawns_visible_text_entities() {
        let mut app = App::new();
        app.add_systems(Update, update_name_labels);

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

        let mut labels = app
            .world_mut()
            .query::<(&NameLabel, &Text, &Node, &Visibility)>();
        let labels = labels.iter(app.world()).collect::<Vec<_>>();

        assert_eq!(labels.len(), NameLabelLayer::ALL.len());
        for (label, text, node, visibility) in labels {
            assert_eq!(label.player, player);
            assert_eq!(label.snake, snake);
            assert_eq!(text.0, "Alice");
            let position = label_screen_position(Vec2::new(10.0, 20.0), label.layer);
            assert_eq!(
                (node.left, node.top),
                (Val::Px(position.x), Val::Px(position.y))
            );
            assert_eq!(*visibility, Visibility::Visible);
        }
    }

    #[test]
    fn update_name_labels_treats_missing_status_as_alive() {
        let mut app = App::new();
        app.add_systems(Update, update_name_labels);

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
            .spawn(Player {
                id: lightyear::prelude::PeerId::Netcode(1),
                name: "Alice".to_string(),
                snake: Some(snake),
            })
            .id();
        app.world_mut().entity_mut(snake).insert(HasPlayer(player));

        app.update();

        let mut labels = app.world_mut().query::<&NameLabel>();
        assert_eq!(labels.iter(app.world()).count(), NameLabelLayer::ALL.len());
    }

    #[test]
    fn update_name_labels_uses_has_player_when_player_snake_is_missing() {
        let mut app = App::new();
        app.add_systems(Update, update_name_labels);

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
                    snake: None,
                },
                PlayerStatus::Alive,
            ))
            .id();
        app.world_mut().entity_mut(snake).insert(HasPlayer(player));

        app.update();

        let mut labels = app.world_mut().query::<(&NameLabel, &Text)>();
        let labels = labels.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(labels.len(), NameLabelLayer::ALL.len());
        assert!(labels
            .iter()
            .all(|(label, text)| label.player == player && text.0 == "Alice"));
    }
}
