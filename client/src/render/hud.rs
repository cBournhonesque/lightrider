use crate::collision::death::DeathView;
use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{Controlled, Predicted};
use shared::config::{ArenaConfig, GameConfig};
use shared::network::protocol::prelude::{
    Player, PlayerDeathStats, PlayerRank, PlayerScore, PlayerStatus, RoomId, TailPoints,
};

const TOP_LEADERBOARD_ROWS: usize = 5;
const NEARBY_LEADERBOARD_ROWS: usize = 5;
const MAX_LEADERBOARD_ROWS: usize = 10;
const MINIMAP_WIDTH: f32 = 180.0;
const MINIMAP_HEIGHT: f32 = 82.0;
const MINIMAP_DOT_SIZE: f32 = 8.0;

pub(crate) struct HudRenderPlugin;

#[derive(Component)]
struct LeaderboardText;

#[derive(Component)]
struct DeathOverlayRoot;

#[derive(Component)]
struct DeathOverlayTitle;

#[derive(Component)]
struct DeathOverlayStatsText;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct MiniMapDot(MiniMapDotKind);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MiniMapDotKind {
    Leader,
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
        app.add_systems(Startup, spawn_hud);
        app.add_systems(Update, (update_leaderboard, update_death_overlay));
        app.add_systems(
            PostUpdate,
            update_minimap.after(FrameInterpolationSystems::Interpolate),
        );
    }
}

fn spawn_hud(mut commands: Commands) {
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
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.018, 0.022, 0.72)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Leaderboard"),
                TextColor(Color::srgb(0.75, 0.95, 1.0)),
                title_font.clone(),
            ));
            parent.spawn((
                LeaderboardText,
                Text::new(""),
                TextColor(Color::srgb(0.86, 0.9, 0.94)),
                body_font.clone(),
            ));
        });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(MINIMAP_WIDTH),
                height: Val::Px(MINIMAP_HEIGHT),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.018, 0.022, 0.72)),
        ))
        .with_children(|parent| {
            parent.spawn((
                MiniMapDot(MiniMapDotKind::Leader),
                minimap_dot_node(),
                BackgroundColor(Color::srgb(1.0, 0.86, 0.26)),
                Visibility::Hidden,
            ));
            parent.spawn((
                MiniMapDot(MiniMapDotKind::Player),
                minimap_dot_node(),
                BackgroundColor(Color::srgb(0.16, 0.78, 1.0)),
                Visibility::Hidden,
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
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.018, 0.022, 0.84)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                DeathOverlayTitle,
                Text::new(""),
                TextColor(Color::srgb(1.0, 0.86, 0.26)),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
            ));
            parent.spawn((
                DeathOverlayStatsText,
                Text::new(""),
                TextColor(Color::srgb(0.86, 0.9, 0.94)),
                body_font.clone(),
            ));
        });
}

fn minimap_dot_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: Val::Px(MINIMAP_DOT_SIZE),
        height: Val::Px(MINIMAP_DOT_SIZE),
        ..default()
    }
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
    config: Res<GameConfig>,
    players: Query<(&Player, &PlayerScore, &PlayerRank, &RoomId, Has<Controlled>)>,
    predicted_tails: Query<&TailPoints, With<Predicted>>,
    tails: Query<&TailPoints>,
    mut dots: Query<(&MiniMapDot, &mut Node, &mut Visibility)>,
) {
    let local_player = players.iter().find(|(_, _, _, _, is_local)| *is_local);
    let local_room = local_player.map(|(_, _, _, room, _)| *room);
    let local_position =
        predicted_tails.single().ok().map(snake_head).or_else(|| {
            local_player.and_then(|(player, _, _, _, _)| player_position(player, &tails))
        });
    let leader_position = local_room.and_then(|room| {
        players
            .iter()
            .filter(|(_, _, _, player_room, _)| **player_room == room)
            .min_by(compare_player_score)
            .and_then(|(player, _, _, _, is_local)| {
                if is_local {
                    local_position
                } else {
                    player_position(player, &tails)
                }
            })
    });

    for (dot, mut node, mut visibility) in &mut dots {
        let position = match dot.0 {
            MiniMapDotKind::Leader => leader_position,
            MiniMapDotKind::Player => local_position,
        };
        if let Some(position) = position {
            let panel_position = minimap_position(position, &config.arena);
            node.left = Val::Px(panel_position.x);
            node.top = Val::Px(panel_position.y);
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
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
    (_, left_score, left_rank, _, _): &(&Player, &PlayerScore, &PlayerRank, &RoomId, bool),
    (_, right_score, right_rank, _, _): &(&Player, &PlayerScore, &PlayerRank, &RoomId, bool),
) -> std::cmp::Ordering {
    compare_rank_score(
        left_score.value,
        left_rank.value,
        right_score.value,
        right_rank.value,
    )
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

fn minimap_position(position: Vec2, arena: &ArenaConfig) -> Vec2 {
    let normalized_x = ((position.x / arena.width) + 0.5).clamp(0.0, 1.0);
    let normalized_y = (0.5 - (position.y / arena.height)).clamp(0.0, 1.0);
    Vec2::new(
        normalized_x * (MINIMAP_WIDTH - MINIMAP_DOT_SIZE),
        normalized_y * (MINIMAP_HEIGHT - MINIMAP_DOT_SIZE),
    )
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
}
