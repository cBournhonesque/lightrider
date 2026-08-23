use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use std::collections::HashSet;

use crate::camera::CameraSystems;
use crate::leaderboard::LeaderboardState;
use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct LeaderRenderPlugin;

const LEADER_CROWN_WIDTH_PX: f32 = 22.0;
const LEADER_CROWN_HEIGHT_PX: f32 = 20.0;
const LEADER_CROWN_OFFSET_PX: f32 = 13.0;
const LEADER_CROWN_Z_INDEX: i32 = 40;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LeaderCrownVisual {
    player: Entity,
}

struct DesiredLeaderCrown {
    player: Entity,
    node: Node,
    image: ImageNode,
}

impl Plugin for LeaderRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            sync_leader_crowns
                .after(FrameInterpolationSystems::Interpolate)
                .after(CameraSystems::Follow),
        );
    }
}

fn sync_leader_crowns(
    mut commands: Commands,
    config: Res<GameConfig>,
    leaderboard_state: Res<LeaderboardState>,
    sheet: Res<PowerlineSpriteSheet>,
    cameras: Query<(&Camera, &Transform), With<Camera2d>>,
    players: Query<(Entity, &Player, &RoomId)>,
    heads: Query<&SnakeHead>,
    mut visuals: Query<(Entity, &LeaderCrownVisual, &mut Node, &mut ImageNode)>,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut visuals {
            commands.entity(entity).despawn();
        }
        return;
    }

    let camera = cameras.single().ok();
    let desired = desired_leader_crowns(&leaderboard_state, &sheet, &players, &heads, camera);
    let mut seen = HashSet::with_capacity(desired.len());

    for desired in desired {
        seen.insert(desired.player);
        let mut updated = false;
        for (_, visual, mut node, mut image) in &mut visuals {
            if visual.player == desired.player {
                *node = desired.node.clone();
                *image = desired.image.clone();
                updated = true;
                break;
            }
        }
        if !updated {
            commands.spawn((
                LeaderCrownVisual {
                    player: desired.player,
                },
                desired.node,
                desired.image,
                ZIndex(LEADER_CROWN_Z_INDEX),
            ));
        }
    }

    for (entity, visual, _, _) in &mut visuals {
        if !seen.contains(&visual.player) {
            commands.entity(entity).despawn();
        }
    }
}

fn desired_leader_crowns(
    leaderboard_state: &LeaderboardState,
    sheet: &PowerlineSpriteSheet,
    players: &Query<(Entity, &Player, &RoomId)>,
    heads: &Query<&SnakeHead>,
    camera: Option<(&Camera, &Transform)>,
) -> Vec<DesiredLeaderCrown> {
    let mut crowns = Vec::new();

    let Some(snapshot) = leaderboard_state.latest() else {
        return crowns;
    };

    for entry in snapshot
        .entries
        .iter()
        .filter(|entry| entry.rank == 1 && entry.status == PlayerStatus::Alive)
    {
        let Ok((player_entity, player, room)) = players.get(entry.player) else {
            continue;
        };
        if *room != snapshot.room {
            continue;
        }
        let Some(snake) = player.snake else {
            continue;
        };
        let Ok(head) = heads.get(snake) else {
            continue;
        };
        let Some(head_viewport_position) = head_viewport_position(head, camera) else {
            continue;
        };

        crowns.push(DesiredLeaderCrown {
            player: player_entity,
            node: crown_node(head_viewport_position),
            image: ImageNode::new(sheet.image()).with_rect(PowerlineFrame::Crown.rect()),
        });
    }

    crowns
}

fn head_viewport_position(head: &SnakeHead, camera: Option<(&Camera, &Transform)>) -> Option<Vec2> {
    let (camera, camera_transform) = camera?;
    let camera_transform = GlobalTransform::from(*camera_transform);
    camera
        .world_to_viewport(&camera_transform, head.position.extend(0.0))
        .ok()
}

fn crown_node(head_viewport_position: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(head_viewport_position.x - LEADER_CROWN_WIDTH_PX * 0.5),
        top: Val::Px(head_viewport_position.y - LEADER_CROWN_HEIGHT_PX - LEADER_CROWN_OFFSET_PX),
        width: Val::Px(LEADER_CROWN_WIDTH_PX),
        height: Val::Px(LEADER_CROWN_HEIGHT_PX),
        ..default()
    }
}
