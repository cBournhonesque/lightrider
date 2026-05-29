use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use std::collections::HashSet;

use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use shared::config::GameConfig;
use shared::network::protocol::prelude::*;

pub(crate) struct LeaderRenderPlugin;

const LEADER_CROWN_Z: f32 = 13.0;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LeaderCrownVisual {
    player: Entity,
}

struct DesiredLeaderCrown {
    player: Entity,
    transform: Transform,
    sprite: Sprite,
}

impl Plugin for LeaderRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            sync_leader_crowns.after(FrameInterpolationSystems::Interpolate),
        );
    }
}

fn sync_leader_crowns(
    mut commands: Commands,
    config: Res<GameConfig>,
    sheet: Res<PowerlineSpriteSheet>,
    players: Query<(Entity, &Player, &PlayerRank, &PlayerStatus, &RoomId)>,
    tails: Query<&TailPoints>,
    mut visuals: Query<(Entity, &LeaderCrownVisual, &mut Transform, &mut Sprite)>,
) {
    if !config.render.use_assets {
        for (entity, _, _, _) in &mut visuals {
            commands.entity(entity).despawn();
        }
        return;
    }

    let desired = desired_leader_crowns(&config, &sheet, &players, &tails);
    let mut seen = HashSet::with_capacity(desired.len());

    for desired in desired {
        seen.insert(desired.player);
        let mut updated = false;
        for (_, visual, mut transform, mut sprite) in &mut visuals {
            if visual.player == desired.player {
                *transform = desired.transform;
                *sprite = desired.sprite.clone();
                updated = true;
                break;
            }
        }
        if !updated {
            commands.spawn((
                LeaderCrownVisual {
                    player: desired.player,
                },
                desired.sprite,
                desired.transform,
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
    config: &GameConfig,
    sheet: &PowerlineSpriteSheet,
    players: &Query<(Entity, &Player, &PlayerRank, &PlayerStatus, &RoomId)>,
    tails: &Query<&TailPoints>,
) -> Vec<DesiredLeaderCrown> {
    let crown_size = Vec2::new(
        config.render.head_size.max(1.0) * 2.6,
        config.render.head_size.max(1.0) * 2.25,
    );
    let crown_offset = Vec2::Y * config.render.head_size.max(1.0) * 1.55;
    let mut crowns = Vec::new();

    for (player_entity, player, rank, status, _) in players.iter() {
        if rank.value != 1 || *status != PlayerStatus::Alive {
            continue;
        }
        let Some(snake) = player.snake else {
            continue;
        };
        let Ok(tail) = tails.get(snake) else {
            continue;
        };

        crowns.push(DesiredLeaderCrown {
            player: player_entity,
            transform: Transform::from_translation(
                (tail.front().0 + crown_offset).extend(LEADER_CROWN_Z),
            ),
            sprite: sheet.sprite(PowerlineFrame::Crown, crown_size, Color::WHITE),
        });
    }

    crowns
}
