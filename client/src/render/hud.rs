use crate::collision::death::DeathView;
use crate::render::assets::{PowerlineFrame, PowerlineSpriteSheet};
use crate::render::colors::snake_color_for_player;
use crate::render::ui_style;
use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Client, Controlled, Link, Predicted};
use shared::config::{ArenaConfig, GameConfig};
use shared::network::protocol::prelude::{
    Player, PlayerDeathStats, PlayerRank, PlayerScore, PlayerStatus, RoomId, TailPoints,
};
use std::{collections::HashSet, time::Duration};

const TOP_LEADERBOARD_ROWS: usize = 5;
const NEARBY_LEADERBOARD_ROWS: usize = 5;
const MAX_LEADERBOARD_ROWS: usize = 10;
const MINIMAP_WIDTH: f32 = 180.0;
const MINIMAP_HEIGHT: f32 = 82.0;
const MINIMAP_DOT_SIZE: f32 = 8.0;
const MINIMAP_TRAIL_THICKNESS: f32 = 2.0;
const MINIMAP_CROWN_WIDTH: f32 = 16.0;
const MINIMAP_CROWN_HEIGHT: f32 = 14.0;
const DEBUG_BUTTON_WIDTH: f32 = 78.0;
const DEBUG_BUTTON_HEIGHT: f32 = 30.0;
const DEBUG_PANEL_WIDTH: f32 = 150.0;

pub(crate) struct HudRenderPlugin;

#[derive(Resource, Default)]
struct DebugPanelState {
    visible: bool,
}

#[derive(Component)]
struct LeaderboardText;

#[derive(Component)]
struct DeathOverlayRoot;

#[derive(Component)]
struct DeathOverlayTitle;

#[derive(Component)]
struct DeathOverlayStatsText;

#[derive(Component)]
struct DebugToggleButton;

#[derive(Component)]
struct DebugPanelRoot;

#[derive(Component)]
struct DebugStatsText;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct MiniMapDot(MiniMapDotKind);

#[derive(Component)]
struct MiniMapRoot;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct MiniMapTrailSegment {
    owner: Entity,
    index: usize,
}

struct DesiredMiniMapTrailSegment {
    owner: Entity,
    index: usize,
    node: Node,
    color: BackgroundColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MiniMapDotKind {
    Leader,
    LeaderCrown,
    Player,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LeaderboardEntry {
    id: u64,
    name: String,
    score: u32,
    rank: u16,
    status: PlayerStatus,
    is_local: bool,
}

impl Plugin for HudRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugPanelState>();
        app.add_systems(Startup, spawn_hud);
        app.add_systems(
            Update,
            (
                update_leaderboard,
                update_death_overlay,
                toggle_debug_panel,
                update_debug_stats,
            ),
        );
        app.add_systems(
            PostUpdate,
            update_minimap.after(FrameInterpolationSystems::Interpolate),
        );
    }
}

fn spawn_hud(mut commands: Commands, sheet: Res<PowerlineSpriteSheet>) {
    let title_font = TextFont {
        font_size: 16.0,
        ..default()
    };
    let body_font = TextFont {
        font_size: 13.0,
        ..default()
    };

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(12.0),
                width: Val::Px(278.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
            ui_style::panel_background(0.54),
            ui_style::panel_border(),
            ui_style::panel_shadow(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Leaderboard"),
                ui_style::title_color(),
                ui_style::text_glow(),
                title_font.clone(),
            ));
            parent.spawn((
                LeaderboardText,
                Text::new(""),
                ui_style::body_color(),
                ui_style::text_glow(),
                body_font.clone(),
            ));
        });

    commands
        .spawn((
            MiniMapRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(MINIMAP_WIDTH),
                height: Val::Px(MINIMAP_HEIGHT),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                ..default()
            },
            ui_style::panel_background(0.54),
            ui_style::panel_border(),
            ui_style::panel_shadow(),
        ))
        .with_children(|parent| {
            parent.spawn((
                MiniMapDot(MiniMapDotKind::Leader),
                minimap_node(Vec2::splat(MINIMAP_DOT_SIZE)),
                BackgroundColor(Color::srgb(1.0, 0.86, 0.26)),
                ZIndex(2),
                Visibility::Hidden,
            ));
            parent.spawn((
                MiniMapDot(MiniMapDotKind::LeaderCrown),
                minimap_node(Vec2::new(MINIMAP_CROWN_WIDTH, MINIMAP_CROWN_HEIGHT)),
                ImageNode::new(sheet.image()).with_rect(PowerlineFrame::Crown.rect()),
                ZIndex(3),
                Visibility::Hidden,
            ));
            parent.spawn((
                MiniMapDot(MiniMapDotKind::Player),
                minimap_node(Vec2::splat(MINIMAP_DOT_SIZE)),
                BackgroundColor(Color::srgb(0.16, 0.78, 1.0)),
                ZIndex(2),
                Visibility::Hidden,
            ));
        });

    commands
        .spawn((
            DebugPanelRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(50.0),
                width: Val::Px(DEBUG_PANEL_WIDTH),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                ..default()
            },
            ui_style::panel_background(0.54),
            ui_style::panel_border(),
            ui_style::panel_shadow(),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Network"),
                ui_style::title_color(),
                ui_style::text_glow(),
                title_font.clone(),
            ));
            parent.spawn((
                DebugStatsText,
                Text::new(format_network_debug_stats(None)),
                ui_style::body_color(),
                ui_style::text_glow(),
                body_font.clone(),
            ));
        });

    commands
        .spawn((
            Button,
            DebugToggleButton,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(DEBUG_BUTTON_WIDTH),
                height: Val::Px(DEBUG_BUTTON_HEIGHT),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                ..default()
            },
            BackgroundColor(debug_button_color(false, Interaction::None)),
            ui_style::button_border(),
            ui_style::panel_shadow(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("DEBUG"),
                ui_style::title_color(),
                ui_style::text_glow(),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
            ));
        });

    commands
        .spawn((
            DeathOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(340.0),
                margin: UiRect {
                    left: Val::Px(-170.0),
                    top: Val::Px(-120.0),
                    ..default()
                },
                border: UiRect::all(Val::Px(1.0)),
                border_radius: ui_style::panel_radius(),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            ui_style::panel_background(0.68),
            ui_style::panel_border(),
            ui_style::panel_shadow(),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                DeathOverlayTitle,
                Text::new(""),
                ui_style::title_color(),
                ui_style::text_glow(),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
            ));
            parent.spawn((
                DeathOverlayStatsText,
                Text::new(""),
                ui_style::body_color(),
                ui_style::text_glow(),
                body_font.clone(),
            ));
        });
}

fn minimap_node(size: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: Val::Px(size.x),
        height: Val::Px(size.y),
        ..default()
    }
}

fn toggle_debug_panel(
    mut state: ResMut<DebugPanelState>,
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<DebugToggleButton>),
    >,
    mut panels: Query<&mut Visibility, With<DebugPanelRoot>>,
) {
    for (interaction, mut color) in &mut buttons {
        if *interaction == Interaction::Pressed {
            state.visible = !state.visible;
            for mut visibility in &mut panels {
                *visibility = if state.visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
        *color = BackgroundColor(debug_button_color(state.visible, *interaction));
    }
}

fn update_debug_stats(
    clients: Query<(&Link, Has<Connected>), With<Client>>,
    mut stats_text: Query<&mut Text, With<DebugStatsText>>,
) {
    let Ok(mut text) = stats_text.single_mut() else {
        return;
    };
    let stats = clients
        .iter()
        .next()
        .map(|(link, connected)| NetworkDebugStats {
            connected,
            ping: link.stats.rtt,
            jitter: link.stats.jitter,
        });

    text.0 = format_network_debug_stats(stats);
}

fn update_leaderboard(
    players: Query<(
        Entity,
        &Player,
        &PlayerScore,
        &PlayerRank,
        &PlayerStatus,
        &RoomId,
        Has<Controlled>,
    )>,
    mut leaderboard: Query<&mut Text, With<LeaderboardText>>,
) {
    let Ok(mut text) = leaderboard.single_mut() else {
        return;
    };

    let local_room = players
        .iter()
        .find_map(|(_, _, _, _, _, room, is_local)| is_local.then_some(*room));
    let mut entries = players
        .iter()
        .filter(|(_, _, _, _, _, room, _)| {
            local_room.map_or(true, |local_room| *room == &local_room)
        })
        .map(
            |(entity, player, score, rank, status, _, is_local)| LeaderboardEntry {
                id: entity.to_bits(),
                name: player.name.clone(),
                score: score.value,
                rank: rank.value,
                status: *status,
                is_local,
            },
        )
        .collect::<Vec<_>>();

    let rows = select_leaderboard_rows(&mut entries);
    text.0 = format_leaderboard_rows(&rows);
}

fn update_minimap(
    mut commands: Commands,
    config: Res<GameConfig>,
    minimap_root: Query<Entity, With<MiniMapRoot>>,
    players: Query<(
        Entity,
        &Player,
        &PlayerScore,
        &PlayerRank,
        &RoomId,
        Has<Controlled>,
    )>,
    predicted_tails: Query<&TailPoints, With<Predicted>>,
    tails: Query<&TailPoints>,
    mut dots: Query<(&MiniMapDot, &mut Node, &mut Visibility), Without<MiniMapTrailSegment>>,
    mut trail_segments: Query<
        (
            Entity,
            &MiniMapTrailSegment,
            &mut Node,
            &mut BackgroundColor,
            &mut Visibility,
        ),
        Without<MiniMapDot>,
    >,
) {
    let local_player = players.iter().find(|(_, _, _, _, _, is_local)| *is_local);
    let local_room = local_player.map(|(_, _, _, _, room, _)| *room);
    let local_tail = predicted_tails.single().ok().or_else(|| {
        local_player
            .and_then(|(_, player, _, _, _, _)| player.snake)
            .and_then(|snake| tails.get(snake).ok())
    });
    let local_position = local_tail.map(snake_head);
    let leader_position = local_room.and_then(|room| {
        players
            .iter()
            .filter(|(_, _, _, _, player_room, _)| **player_room == room)
            .min_by(compare_player_score)
            .and_then(|(_, player, _, _, _, is_local)| {
                if is_local {
                    local_position
                } else {
                    player_position(player, &tails)
                }
            })
    });

    for (dot, mut node, mut visibility) in &mut dots {
        let position = match dot.0 {
            MiniMapDotKind::Leader | MiniMapDotKind::LeaderCrown => leader_position,
            MiniMapDotKind::Player => local_position,
        };
        if let Some(position) = position {
            let panel_position = minimap_position_for_kind(position, &config.arena, dot.0);
            node.left = Val::Px(panel_position.x);
            node.top = Val::Px(panel_position.y);
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    sync_minimap_trail(
        &mut commands,
        minimap_root.single().ok(),
        desired_minimap_trails(&players, local_room, local_tail, &tails, &config.arena),
        &mut trail_segments,
    );
}

fn sync_minimap_trail(
    commands: &mut Commands,
    minimap_root: Option<Entity>,
    desired: Vec<DesiredMiniMapTrailSegment>,
    trail_segments: &mut Query<
        (
            Entity,
            &MiniMapTrailSegment,
            &mut Node,
            &mut BackgroundColor,
            &mut Visibility,
        ),
        Without<MiniMapDot>,
    >,
) {
    let mut seen = HashSet::with_capacity(desired.len());

    for desired in desired {
        let key = (desired.owner, desired.index);
        seen.insert(key);
        let mut updated = false;
        for (_, segment, mut existing_node, mut background, mut visibility) in
            trail_segments.iter_mut()
        {
            if segment.owner == desired.owner && segment.index == desired.index {
                *existing_node = desired.node.clone();
                *background = desired.color;
                *visibility = Visibility::Inherited;
                updated = true;
                break;
            }
        }
        if !updated {
            let Some(root) = minimap_root else {
                continue;
            };
            commands.entity(root).with_children(|parent| {
                parent.spawn((
                    MiniMapTrailSegment {
                        owner: desired.owner,
                        index: desired.index,
                    },
                    desired.node,
                    desired.color,
                    ZIndex(-1),
                    Visibility::Inherited,
                ));
            });
        }
    }

    for (entity, segment, _, _, mut visibility) in trail_segments.iter_mut() {
        if !seen.contains(&(segment.owner, segment.index)) {
            *visibility = Visibility::Hidden;
            commands.entity(entity).despawn();
        }
    }
}

fn update_death_overlay(
    death_view: Res<DeathView>,
    time: Res<Time>,
    mut root: Query<&mut Visibility, With<DeathOverlayRoot>>,
    mut title: Query<&mut Text, (With<DeathOverlayTitle>, Without<DeathOverlayStatsText>)>,
    mut stats_text: Query<&mut Text, (With<DeathOverlayStatsText>, Without<DeathOverlayTitle>)>,
) {
    let Ok(mut visibility) = root.single_mut() else {
        return;
    };
    let Some(stats) = death_view.stats else {
        *visibility = Visibility::Hidden;
        return;
    };

    *visibility = Visibility::Inherited;
    if let Ok(mut title) = title.single_mut() {
        title.0 = death_view.message.clone();
    }
    if let Ok(mut text) = stats_text.single_mut() {
        let respawn_wait = (death_view.respawn_allowed_at_seconds - time.elapsed_secs()).max(0.0);
        text.0 = format_death_stats(stats, respawn_wait);
    }
}

fn compare_player_score(
    (_, _, left_score, left_rank, _, _): &(
        Entity,
        &Player,
        &PlayerScore,
        &PlayerRank,
        &RoomId,
        bool,
    ),
    (_, _, right_score, right_rank, _, _): &(
        Entity,
        &Player,
        &PlayerScore,
        &PlayerRank,
        &RoomId,
        bool,
    ),
) -> std::cmp::Ordering {
    compare_rank_score(
        left_score.value,
        left_rank.value,
        right_score.value,
        right_rank.value,
    )
}

fn desired_minimap_trails(
    players: &Query<(
        Entity,
        &Player,
        &PlayerScore,
        &PlayerRank,
        &RoomId,
        Has<Controlled>,
    )>,
    local_room: Option<RoomId>,
    local_tail: Option<&TailPoints>,
    tails: &Query<&TailPoints>,
    arena: &ArenaConfig,
) -> Vec<DesiredMiniMapTrailSegment> {
    let mut desired = Vec::new();
    let Some(local_room) = local_room else {
        return desired;
    };

    for (player_entity, player, _, _, room, is_local) in players.iter() {
        if *room != local_room {
            continue;
        }
        let tail = if is_local {
            local_tail
        } else {
            player.snake.and_then(|snake| tails.get(snake).ok())
        };
        let Some(tail) = tail else {
            continue;
        };
        let color = minimap_trail_color(player, is_local);
        desired.extend(
            minimap_trail_nodes(tail, arena)
                .into_iter()
                .map(|(index, node)| DesiredMiniMapTrailSegment {
                    owner: player_entity,
                    index,
                    node,
                    color,
                }),
        );
    }

    desired
}

fn player_position(player: &Player, tails: &Query<&TailPoints>) -> Option<Vec2> {
    player
        .snake
        .and_then(|snake| tails.get(snake).ok())
        .map(snake_head)
}

fn snake_head(tail: &TailPoints) -> Vec2 {
    tail.front().0
}

#[cfg(test)]
fn minimap_position(position: Vec2, arena: &ArenaConfig) -> Vec2 {
    minimap_position_for_size(position, arena, Vec2::splat(MINIMAP_DOT_SIZE), Vec2::ZERO)
}

fn minimap_position_for_kind(position: Vec2, arena: &ArenaConfig, kind: MiniMapDotKind) -> Vec2 {
    match kind {
        MiniMapDotKind::Leader => {
            minimap_position_for_size(position, arena, Vec2::splat(MINIMAP_DOT_SIZE), Vec2::ZERO)
        }
        MiniMapDotKind::LeaderCrown => minimap_position_for_size(
            position,
            arena,
            Vec2::new(MINIMAP_CROWN_WIDTH, MINIMAP_CROWN_HEIGHT),
            Vec2::new(0.0, -MINIMAP_CROWN_HEIGHT + 3.0),
        ),
        MiniMapDotKind::Player => {
            minimap_position_for_size(position, arena, Vec2::splat(MINIMAP_DOT_SIZE), Vec2::ZERO)
        }
    }
}

fn minimap_position_for_size(
    position: Vec2,
    arena: &ArenaConfig,
    size: Vec2,
    offset: Vec2,
) -> Vec2 {
    let normalized_x = ((position.x / arena.width) + 0.5).clamp(0.0, 1.0);
    let normalized_y = (0.5 - (position.y / arena.height)).clamp(0.0, 1.0);
    let max = Vec2::new(MINIMAP_WIDTH - size.x, MINIMAP_HEIGHT - size.y).max(Vec2::ZERO);
    (Vec2::new(normalized_x * max.x, normalized_y * max.y) + offset).clamp(Vec2::ZERO, max)
}

fn minimap_trail_nodes(tail: &TailPoints, arena: &ArenaConfig) -> Vec<(usize, Node)> {
    tail.pairs_front_to_back()
        .enumerate()
        .filter_map(|(index, (start, end))| {
            let start = minimap_position_for_size(start.0, arena, Vec2::ZERO, Vec2::ZERO);
            let end = minimap_position_for_size(end.0, arena, Vec2::ZERO, Vec2::ZERO);
            minimap_trail_node(start, end).map(|node| (index, node))
        })
        .collect()
}

fn minimap_trail_node(start: Vec2, end: Vec2) -> Option<Node> {
    let delta = end - start;
    if delta.length_squared() <= f32::EPSILON {
        return None;
    }
    let thickness = MINIMAP_TRAIL_THICKNESS;
    let horizontal = delta.x.abs() >= delta.y.abs();
    let (left, top, width, height) = if horizontal {
        (
            start.x.min(end.x),
            start.y - thickness * 0.5,
            delta.x.abs().max(thickness),
            thickness,
        )
    } else {
        (
            start.x - thickness * 0.5,
            start.y.min(end.y),
            thickness,
            delta.y.abs().max(thickness),
        )
    };
    let left = left.clamp(0.0, MINIMAP_WIDTH);
    let top = top.clamp(0.0, MINIMAP_HEIGHT);
    Some(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(left),
        top: Val::Px(top),
        width: Val::Px(width.min((MINIMAP_WIDTH - left).max(0.0))),
        height: Val::Px(height.min((MINIMAP_HEIGHT - top).max(0.0))),
        ..default()
    })
}

fn minimap_trail_color(player: &Player, is_local: bool) -> BackgroundColor {
    let mut color = snake_color_for_player(player).label();
    color.set_alpha(if is_local { 0.78 } else { 0.48 });
    BackgroundColor(color)
}

fn select_leaderboard_rows(entries: &mut [LeaderboardEntry]) -> Vec<LeaderboardEntry> {
    entries.sort_by(|left, right| {
        compare_rank_score(left.score, left.rank, right.score, right.rank)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut rows = Vec::with_capacity(MAX_LEADERBOARD_ROWS);
    for entry in entries.iter().take(TOP_LEADERBOARD_ROWS) {
        push_unique(&mut rows, entry);
    }

    if let Some(local_index) = entries.iter().position(|entry| entry.is_local) {
        let start = nearby_start(local_index, entries.len(), NEARBY_LEADERBOARD_ROWS);
        for entry in entries.iter().skip(start).take(NEARBY_LEADERBOARD_ROWS) {
            push_unique(&mut rows, entry);
        }
    } else {
        for entry in entries.iter().skip(TOP_LEADERBOARD_ROWS) {
            push_unique(&mut rows, entry);
            if rows.len() >= MAX_LEADERBOARD_ROWS {
                break;
            }
        }
    }

    rows.truncate(MAX_LEADERBOARD_ROWS);
    rows
}

fn compare_rank_score(
    left_score: u32,
    left_rank: u16,
    right_score: u32,
    right_rank: u16,
) -> std::cmp::Ordering {
    match (nonzero_rank(left_rank), nonzero_rank(right_rank)) {
        (Some(left_rank), Some(right_rank)) => left_rank.cmp(&right_rank),
        _ => right_score.cmp(&left_score),
    }
}

fn nonzero_rank(rank: u16) -> Option<u16> {
    (rank != 0).then_some(rank)
}

fn nearby_start(local_index: usize, len: usize, count: usize) -> usize {
    if len <= count {
        0
    } else {
        local_index.saturating_sub(count / 2).min(len - count)
    }
}

fn push_unique(rows: &mut Vec<LeaderboardEntry>, entry: &LeaderboardEntry) {
    if rows.len() < MAX_LEADERBOARD_ROWS && rows.iter().all(|row| row.id != entry.id) {
        rows.push(entry.clone());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NetworkDebugStats {
    connected: bool,
    ping: Duration,
    jitter: Duration,
}

fn debug_button_color(visible: bool, interaction: Interaction) -> Color {
    ui_style::button_background(visible, interaction)
}

fn format_network_debug_stats(stats: Option<NetworkDebugStats>) -> String {
    let Some(stats) = stats else {
        return "Status: offline\nPing: --\nJitter: --".to_string();
    };
    if !stats.connected {
        return "Status: connecting\nPing: --\nJitter: --".to_string();
    }

    format!(
        "Status: connected\nPing: {}\nJitter: {}",
        format_latency(stats.ping),
        format_latency(stats.jitter)
    )
}

fn format_latency(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1000.0;
    if millis < 10.0 {
        format!("{millis:.1} ms")
    } else {
        format!("{millis:.0} ms")
    }
}

fn format_leaderboard_rows(rows: &[LeaderboardEntry]) -> String {
    if rows.is_empty() {
        return "No players".to_string();
    }

    let mut text = String::from("RANK NAME             SCORE\n");
    for (index, row) in rows.iter().enumerate() {
        let rank = if row.rank == 0 {
            index as u16 + 1
        } else {
            row.rank
        };
        let marker = if row.is_local { ">" } else { " " };
        let status = if row.status == PlayerStatus::Dead {
            "x"
        } else {
            " "
        };
        text.push_str(&format!(
            "{marker}{rank:>3} {status}{:<14} {:>6}\n",
            truncate_name(&row.name, 14),
            row.score
        ));
    }
    text
}

fn format_death_stats(stats: PlayerDeathStats, respawn_wait_seconds: f32) -> String {
    format!(
        "Score: {}\nAverage speed: {:.2}\nTime alive: {}\nKills: {}\nTime as leader: {}\nFood eaten: {}\nRespawn in: {:.1}s",
        stats.score,
        stats.average_speed,
        format_duration(stats.time_alive_seconds),
        stats.kills,
        format_duration(stats.time_as_leader_seconds),
        stats.food_eaten,
        respawn_wait_seconds,
    )
}

fn format_duration(seconds: f32) -> String {
    let total_seconds = seconds.max(0.0).round() as u32;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    let mut chars = name.chars();
    let mut truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() && max_chars > 1 {
        truncated.pop();
        truncated.push('~');
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, score: u32, is_local: bool) -> LeaderboardEntry {
        LeaderboardEntry {
            id,
            name: format!("Player {id}"),
            score,
            rank: id as u16,
            status: PlayerStatus::Alive,
            is_local,
        }
    }

    #[test]
    fn leaderboard_selects_top_and_local_neighbors() {
        let mut entries = (1..=12)
            .map(|rank| entry(rank, 100 - rank as u32, rank == 9))
            .collect::<Vec<_>>();

        let rows = select_leaderboard_rows(&mut entries);
        let ids = rows.iter().map(|entry| entry.id).collect::<Vec<_>>();

        assert_eq!(ids, vec![1, 2, 3, 4, 5, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn leaderboard_is_capped_at_ten_rows_without_local_player() {
        let mut entries = (1..=12)
            .map(|rank| entry(rank, 100 - rank as u32, false))
            .collect::<Vec<_>>();

        assert_eq!(select_leaderboard_rows(&mut entries).len(), 10);
    }

    #[test]
    fn minimap_maps_world_center_to_panel_center() {
        let arena = ArenaConfig {
            width: 1000.0,
            height: 500.0,
        };

        assert_eq!(
            minimap_position(Vec2::ZERO, &arena),
            Vec2::new(
                (MINIMAP_WIDTH - MINIMAP_DOT_SIZE) * 0.5,
                (MINIMAP_HEIGHT - MINIMAP_DOT_SIZE) * 0.5
            )
        );
    }

    #[test]
    fn death_stats_text_contains_requested_stats() {
        let text = format_death_stats(
            PlayerDeathStats {
                average_speed: 1.5,
                score: 240,
                time_alive_seconds: 62.0,
                kills: 3,
                time_as_leader_seconds: 5.0,
                food_eaten: 7,
            },
            1.2,
        );

        assert!(text.contains("Score: 240"));
        assert!(text.contains("Average speed: 1.50"));
        assert!(text.contains("Time alive: 1:02"));
        assert!(text.contains("Kills: 3"));
        assert!(text.contains("Time as leader: 0:05"));
        assert!(text.contains("Food eaten: 7"));
    }

    #[test]
    fn network_debug_stats_show_connection_state_and_latency() {
        let text = format_network_debug_stats(Some(NetworkDebugStats {
            connected: true,
            ping: Duration::from_millis(42),
            jitter: Duration::from_micros(3500),
        }));

        assert!(text.contains("Status: connected"));
        assert!(text.contains("Ping: 42 ms"));
        assert!(text.contains("Jitter: 3.5 ms"));
    }

    #[test]
    fn network_debug_stats_show_connecting_without_latency() {
        assert_eq!(
            format_network_debug_stats(Some(NetworkDebugStats {
                connected: false,
                ping: Duration::ZERO,
                jitter: Duration::ZERO,
            })),
            "Status: connecting\nPing: --\nJitter: --"
        );
    }
}
