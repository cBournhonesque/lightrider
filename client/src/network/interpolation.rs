use std::collections::VecDeque;

use bevy::log::trace;
use bevy::prelude::*;
use lightyear::frame_interpolation::{FrameInterpolate, FrameInterpolationPlugin};
use lightyear::prelude::*;

use shared::network::protocol::prelude::*;

const GEOMETRY_EPSILON: f32 = 0.001;

pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameInterpolationPlugin);
        app.add_systems(
            Update,
            (
                add_frame_interpolation_to_predicted_snakes,
                interpolate_remote_snakes.in_set(InterpolationSystems::Interpolate),
            ),
        );
    }
}

fn add_frame_interpolation_to_predicted_snakes(
    mut commands: Commands,
    snakes: Query<
        Entity,
        (
            With<Predicted>,
            Or<(With<SnakeHead>, With<TailLength>)>,
            Without<FrameInterpolate>,
        ),
    >,
) {
    for snake in &snakes {
        commands.entity(snake).insert(FrameInterpolate);
    }
}

#[derive(Clone, Debug)]
struct SnakeHistorySample {
    start_tick: Tick,
    end_tick: Tick,
    start_head: SnakeHead,
    end_head: SnakeHead,
    start_length: TailLength,
    end_length: TailLength,
    end_tail: TailPoints,
    fraction: f32,
}

fn interpolate_remote_snakes(
    timeline: Single<&InterpolationTimeline, With<IsSynced<InterpolationTimeline>>>,
    mut snakes: Query<
        (
            Entity,
            &mut SnakeHead,
            &mut TailPoints,
            &mut TailLength,
            &ConfirmedHistory<SnakeHead>,
            &ConfirmedHistory<TailPoints>,
            &ConfirmedHistory<TailLength>,
        ),
        With<Interpolated>,
    >,
) {
    let interpolation_tick = timeline.now().tick();
    let interpolation_overstep = timeline.overstep().to_f32();

    for (
        entity,
        mut live_head,
        mut live_tail,
        mut live_length,
        head_history,
        tail_history,
        length_history,
    ) in &mut snakes
    {
        let Some(sample) = sample_snake_history_pair(
            head_history,
            tail_history,
            length_history,
            interpolation_tick,
            interpolation_overstep,
        ) else {
            continue;
        };

        let interpolated_length = interpolate_tail_length(
            sample.start_length.clone(),
            sample.end_length.clone(),
            sample.fraction,
        );
        let Some((interpolated_head, interpolated_tail)) = interpolate_snake_on_confirmed_path(
            &sample.start_head,
            &sample.end_head,
            &sample.end_tail,
            &sample.end_length,
            sample.fraction,
        ) else {
            trace!(
                target: "lightyear_debug::lightrider_interpolation",
                kind = "remote_snake_interpolation_path_mismatch",
                ?entity,
                interpolation_tick = interpolation_tick.0,
                interpolation_overstep,
                sample_start_tick = sample.start_tick.0,
                sample_end_tick = sample.end_tick.0,
                sample_fraction = sample.fraction,
                start_head = ?sample.start_head,
                end_head = ?sample.end_head,
                end_tail_turns = ?sample.end_tail.turns,
                "remote snake interpolation could not build path between samples"
            );
            continue;
        };

        log_diagonal_interpolation(
            entity,
            interpolation_tick,
            interpolation_overstep,
            &sample,
            &interpolated_head,
            &interpolated_tail,
            &interpolated_length,
            &sample.end_tail,
            tail_history,
            sample.end_tick,
        );

        *live_head = interpolated_head;
        *live_tail = interpolated_tail;
        *live_length = interpolated_length;
    }
}

fn sample_snake_history_pair(
    head_history: &ConfirmedHistory<SnakeHead>,
    tail_history: &ConfirmedHistory<TailPoints>,
    length_history: &ConfirmedHistory<TailLength>,
    interpolation_tick: Tick,
    interpolation_overstep: f32,
) -> Option<SnakeHistorySample> {
    let start_index =
        tail_sample_indices(tail_history, |tick| tick <= interpolation_tick).last()?;
    let end_index = tail_sample_indices(tail_history, |tick| tick > interpolation_tick).next();

    let (start_tick, start_head, start_length, start_tail) =
        tail_driven_sample_at_index(head_history, tail_history, length_history, start_index)?;
    let Some((end_tick, end_head, end_length, end_tail)) = end_index.and_then(|index| {
        tail_driven_sample_at_index(head_history, tail_history, length_history, index)
    }) else {
        return Some(SnakeHistorySample {
            start_tick,
            end_tick: start_tick,
            start_head,
            end_head: start_head,
            start_length: start_length.clone(),
            end_length: start_length,
            end_tail: start_tail,
            fraction: 0.0,
        });
    };

    let fraction = if end_tick == start_tick {
        0.0
    } else {
        (((interpolation_tick - start_tick) as f32 + interpolation_overstep)
            / (end_tick - start_tick) as f32)
            .clamp(0.0, 1.0)
    };

    Some(SnakeHistorySample {
        start_tick,
        end_tick,
        start_head,
        end_head,
        start_length,
        end_length,
        end_tail,
        fraction,
    })
}

fn tail_sample_indices<'a>(
    tail_history: &'a ConfirmedHistory<TailPoints>,
    tick_filter: impl Fn(Tick) -> bool + 'a,
) -> impl Iterator<Item = usize> + 'a {
    (0..tail_history.len()).filter(move |index| {
        let Some(tick) = tail_history.get_nth_tick(*index) else {
            return false;
        };
        tick_filter(tick)
            && tail_history
                .get_state_at(tick)
                .is_some_and(|state| matches!(state, HistoryState::Updated(_)))
    })
}

fn tail_driven_sample_at_index(
    head_history: &ConfirmedHistory<SnakeHead>,
    tail_history: &ConfirmedHistory<TailPoints>,
    length_history: &ConfirmedHistory<TailLength>,
    index: usize,
) -> Option<(Tick, SnakeHead, TailLength, TailPoints)> {
    let (tick, HistoryState::Updated(tail)) = tail_history.get_nth_state(index)? else {
        return None;
    };
    let head = *head_history.get_present(tick)?;
    let length = length_history.get_present(tick)?.clone();
    Some((tick, head, length, tail.clone()))
}

fn log_diagonal_interpolation(
    entity: Entity,
    interpolation_tick: Tick,
    interpolation_overstep: f32,
    sample: &SnakeHistorySample,
    interpolated_head: &SnakeHead,
    interpolated_tail: &TailPoints,
    interpolated_length: &TailLength,
    end_tail: &TailPoints,
    tail_history: &ConfirmedHistory<TailPoints>,
    tail_sample_tick: Tick,
) {
    let polyline = interpolated_tail.polyline(interpolated_head, interpolated_length.current_size);
    let Some((segment_index, dx, dy)) = first_diagonal_segment(&polyline) else {
        return;
    };
    let tail_history_ticks = (0..tail_history.len())
        .filter_map(|index| tail_history.get_nth_tick(index))
        .map(|tick| tick.0)
        .collect::<Vec<_>>();
    trace!(
        target: "lightyear_debug::lightrider_interpolation",
        kind = "remote_snake_interpolation_diagonal",
        ?entity,
        interpolation_tick = interpolation_tick.0,
        interpolation_overstep,
        sample_start_tick = sample.start_tick.0,
        sample_end_tick = sample.end_tick.0,
        sample_fraction = sample.fraction,
        head_start = ?sample.start_head,
        head_end = ?sample.end_head,
        interpolated_head = ?interpolated_head,
        tail_sample_tick = tail_sample_tick.0,
        tail_history_ticks = ?tail_history_ticks,
        end_tail_turns = ?end_tail.turns,
        tail_turns = ?interpolated_tail.turns,
        tail_length = interpolated_length.current_size,
        segment_index,
        dx,
        dy,
        "remote snake interpolation produced diagonal tail"
    );
}

fn first_diagonal_segment(polyline: &TailPolyline) -> Option<(usize, f32, f32)> {
    polyline
        .pairs_front_to_back()
        .enumerate()
        .find_map(|(index, (start, end))| {
            let dx = (start.0.x - end.0.x).abs();
            let dy = (start.0.y - end.0.y).abs();
            (!axis_aligned(start.0, end.0)).then_some((index, dx, dy))
        })
}

fn interpolate_snake_on_confirmed_path(
    start: &SnakeHead,
    end: &SnakeHead,
    end_tail: &TailPoints,
    end_length: &TailLength,
    fraction: f32,
) -> Option<(SnakeHead, TailPoints)> {
    let final_polyline = end_tail.polyline(end, end_length.current_size);
    if first_diagonal_segment(&final_polyline).is_some() {
        return None;
    }
    let head_path = head_path_from_final_polyline(start, end, &final_polyline)?;
    let interpolated_head = walk_path(&head_path, fraction, *end);
    let interpolated_tail = tail_points_behind_head(&final_polyline, &interpolated_head)?;

    Some((interpolated_head, interpolated_tail))
}

fn head_path_from_final_polyline(
    start: &SnakeHead,
    end: &SnakeHead,
    final_polyline: &TailPolyline,
) -> Option<Vec<Vec2>> {
    let positions = final_polyline
        .0
        .iter()
        .map(|(position, _)| *position)
        .collect::<Vec<_>>();

    for segment_index in 0..positions.len().saturating_sub(1) {
        if !segment_contains_point(
            positions[segment_index],
            positions[segment_index + 1],
            start.position,
        ) {
            continue;
        }

        let mut path = vec![start.position];
        if !same_position(start.position, positions[segment_index]) {
            path.push(positions[segment_index]);
        }
        for point in positions[..segment_index].iter().rev() {
            if path
                .last()
                .is_none_or(|previous| !same_position(*previous, *point))
            {
                path.push(*point);
            }
        }
        if path
            .last()
            .is_none_or(|previous| !same_position(*previous, end.position))
        {
            path.push(end.position);
        }
        return Some(path);
    }

    None
}

fn walk_path(path: &[Vec2], fraction: f32, end: SnakeHead) -> SnakeHead {
    let total_length = path_length(path);
    walk_path_distance(path, total_length * fraction.clamp(0.0, 1.0), end)
}

fn walk_path_distance(path: &[Vec2], distance: f32, end: SnakeHead) -> SnakeHead {
    if path.len() < 2 {
        return end;
    }
    let total_length = path_length(path);
    if total_length <= GEOMETRY_EPSILON {
        return end;
    }

    let mut remaining = distance.clamp(0.0, total_length);
    for segment_index in 0..path.len() - 1 {
        let from = path[segment_index];
        let to = path[segment_index + 1];
        let segment_length = from.distance(to);
        if segment_length <= GEOMETRY_EPSILON {
            continue;
        }

        if remaining <= segment_length + GEOMETRY_EPSILON {
            let t = (remaining / segment_length).clamp(0.0, 1.0);
            let position = from.lerp(to, t);
            let direction = if t >= 1.0 - GEOMETRY_EPSILON {
                path.get(segment_index + 2)
                    .and_then(|next| direction_between_points(to, *next))
                    .or_else(|| direction_between_points(from, to))
                    .unwrap_or(end.direction)
            } else {
                direction_between_points(from, to).unwrap_or(end.direction)
            };
            return SnakeHead {
                position,
                direction,
            };
        }

        remaining -= segment_length;
    }

    end
}

fn tail_points_behind_head(
    final_polyline: &TailPolyline,
    interpolated_head: &SnakeHead,
) -> Option<TailPoints> {
    let positions = final_polyline
        .0
        .iter()
        .map(|(position, _)| *position)
        .collect::<Vec<_>>();

    for segment_index in 0..positions.len().saturating_sub(1) {
        if !segment_contains_point(
            positions[segment_index],
            positions[segment_index + 1],
            interpolated_head.position,
        ) {
            continue;
        }

        let mut tail_positions = vec![interpolated_head.position];
        if !same_position(interpolated_head.position, positions[segment_index + 1]) {
            tail_positions.push(positions[segment_index + 1]);
        }
        tail_positions.extend_from_slice(&positions[segment_index + 2..]);
        return Some(tail_points_from_tailward_positions(
            interpolated_head,
            &tail_positions,
        ));
    }

    None
}

fn tail_points_from_tailward_positions(head: &SnakeHead, positions: &[Vec2]) -> TailPoints {
    if positions.len() < 2 {
        return TailPoints::empty();
    }

    let mut turns = VecDeque::new();
    let mut current_tailward_direction = head.direction.opposite();
    for segment_index in 0..positions.len() - 1 {
        let Some(segment_tailward_direction) =
            direction_between_points(positions[segment_index], positions[segment_index + 1])
        else {
            continue;
        };
        if segment_tailward_direction != current_tailward_direction {
            turns.push_back(TailTurn::new(
                positions[segment_index],
                segment_tailward_direction,
            ));
            current_tailward_direction = segment_tailward_direction;
        }
    }

    TailPoints::new(turns)
}

fn path_length(path: &[Vec2]) -> f32 {
    path.windows(2)
        .map(|points| points[0].distance(points[1]))
        .sum()
}

fn segment_contains_point(start: Vec2, end: Vec2, point: Vec2) -> bool {
    if same_position(start, end) {
        return same_position(start, point);
    }
    if (start.x - end.x).abs() <= GEOMETRY_EPSILON {
        return (point.x - start.x).abs() <= GEOMETRY_EPSILON
            && point.y >= start.y.min(end.y) - GEOMETRY_EPSILON
            && point.y <= start.y.max(end.y) + GEOMETRY_EPSILON;
    }
    if (start.y - end.y).abs() <= GEOMETRY_EPSILON {
        return (point.y - start.y).abs() <= GEOMETRY_EPSILON
            && point.x >= start.x.min(end.x) - GEOMETRY_EPSILON
            && point.x <= start.x.max(end.x) + GEOMETRY_EPSILON;
    }

    let segment = end - start;
    let projection = (point - start).dot(segment) / segment.length_squared();
    if !(0.0..=1.0).contains(&projection) {
        return false;
    }
    let closest = start + segment * projection;
    closest.distance(point) <= GEOMETRY_EPSILON
}

fn direction_between_points(start: Vec2, end: Vec2) -> Option<Direction> {
    let delta = end - start;
    if delta.length_squared() <= GEOMETRY_EPSILON * GEOMETRY_EPSILON {
        return None;
    }
    if delta.x.abs() >= delta.y.abs() {
        if delta.x >= 0.0 {
            Some(Direction::Right)
        } else {
            Some(Direction::Left)
        }
    } else if delta.y >= 0.0 {
        Some(Direction::Up)
    } else {
        Some(Direction::Down)
    }
}

fn axis_aligned(start: Vec2, end: Vec2) -> bool {
    (start.x - end.x).abs() <= GEOMETRY_EPSILON || (start.y - end.y).abs() <= GEOMETRY_EPSILON
}

fn same_position(a: Vec2, b: Vec2) -> bool {
    a.distance_squared(b) <= GEOMETRY_EPSILON * GEOMETRY_EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    use lightyear::core::time::{Overstep, TickInstant};

    fn insert_timeline(app: &mut App, tick: Tick, overstep: f32) {
        let mut timeline = InterpolationTimeline::default();
        timeline.set_now(TickInstant::from_tick_and_overstep(
            tick,
            Overstep::from_f32(overstep),
        ));
        app.world_mut()
            .spawn((timeline, IsSynced::<InterpolationTimeline>::default()));
    }

    fn head(position: Vec2, direction: Direction) -> SnakeHead {
        SnakeHead {
            position,
            direction,
        }
    }

    fn length(current_size: f32) -> TailLength {
        TailLength {
            current_size,
            target_size: current_size,
        }
    }

    fn confirmed_history<C: Clone + PartialEq>(start: C, end: C) -> ConfirmedHistory<C> {
        let mut history = ConfirmedHistory::default();
        history.insert_present(Tick(0), start);
        history.insert_present(Tick(2), end);
        history
    }

    fn confirmed_history_at<C: Clone + PartialEq>(
        samples: impl IntoIterator<Item = (Tick, C)>,
    ) -> ConfirmedHistory<C> {
        let mut history = ConfirmedHistory::default();
        for (tick, value) in samples {
            history.insert_present(tick, value);
        }
        history
    }

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert!(
            actual.distance(expected) <= GEOMETRY_EPSILON,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn remote_snake_interpolation_updates_straight_head_length_and_tail() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(1), 0.0);

        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                SnakeHead::default(),
                TailPoints::empty(),
                TailLength::default(),
                confirmed_history(
                    head(Vec2::ZERO, Direction::Right),
                    head(Vec2::new(20.0, 0.0), Direction::Right),
                ),
                confirmed_history(TailPoints::empty(), TailPoints::empty()),
                confirmed_history(length(80.0), length(100.0)),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        let live_tail = entity.get::<TailPoints>().unwrap();
        let live_length = entity.get::<TailLength>().unwrap();
        assert_vec2_close(live_head.position, Vec2::new(10.0, 0.0));
        assert_eq!(live_head.direction, Direction::Right);
        assert!(live_tail.turns.is_empty());
        assert_eq!(live_length.current_size, 90.0);
    }

    #[test]
    fn remote_snake_interpolation_walks_turning_path() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(1), 0.5);

        let end_tail = TailPoints::new(VecDeque::from([TailTurn::new(
            Vec2::new(10.0, 0.0),
            Direction::Left,
        )]));
        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                SnakeHead::default(),
                TailPoints::empty(),
                TailLength::default(),
                confirmed_history(
                    head(Vec2::ZERO, Direction::Right),
                    head(Vec2::new(10.0, 10.0), Direction::Up),
                ),
                confirmed_history(TailPoints::empty(), end_tail),
                confirmed_history(length(40.0), length(40.0)),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        let live_tail = entity.get::<TailPoints>().unwrap();
        assert_vec2_close(live_head.position, Vec2::new(10.0, 5.0));
        assert_eq!(live_head.direction, Direction::Up);
        assert_eq!(
            live_tail.turns,
            VecDeque::from([TailTurn::new(Vec2::new(10.0, 0.0), Direction::Left)])
        );
    }

    #[test]
    fn remote_snake_interpolation_preserves_turn_at_current_head() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(1), 0.0);

        let end_tail = TailPoints::new(VecDeque::from([TailTurn::new(
            Vec2::new(10.0, 0.0),
            Direction::Left,
        )]));
        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                SnakeHead::default(),
                TailPoints::empty(),
                TailLength::default(),
                confirmed_history(
                    head(Vec2::ZERO, Direction::Right),
                    head(Vec2::new(10.0, 10.0), Direction::Up),
                ),
                confirmed_history(TailPoints::empty(), end_tail),
                confirmed_history(length(40.0), length(40.0)),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        let live_tail = entity.get::<TailPoints>().unwrap();
        assert_vec2_close(live_head.position, Vec2::new(10.0, 0.0));
        assert_eq!(live_head.direction, Direction::Up);
        assert_eq!(
            live_tail.turns,
            VecDeque::from([TailTurn::new(Vec2::new(10.0, 0.0), Direction::Left)])
        );
    }

    #[test]
    fn remote_snake_interpolation_skips_incoherent_grouped_tail_sample() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(1), 0.0);

        let stale_tail = TailPoints::new(VecDeque::from([TailTurn::new(
            Vec2::ZERO,
            Direction::Right,
        )]));
        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                head(Vec2::new(99.0, 99.0), Direction::Left),
                TailPoints::new(VecDeque::from([TailTurn::new(
                    Vec2::new(90.0, 99.0),
                    Direction::Down,
                )])),
                length(25.0),
                confirmed_history(
                    head(Vec2::new(5.0, 10.0), Direction::Right),
                    head(Vec2::new(10.0, 10.0), Direction::Right),
                ),
                confirmed_history(TailPoints::empty(), stale_tail),
                confirmed_history(length(100.0), length(100.0)),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        let live_tail = entity.get::<TailPoints>().unwrap();
        let live_length = entity.get::<TailLength>().unwrap();
        assert_vec2_close(live_head.position, Vec2::new(99.0, 99.0));
        assert_eq!(live_head.direction, Direction::Left);
        assert_eq!(
            live_tail.turns,
            VecDeque::from([TailTurn::new(Vec2::new(90.0, 99.0), Direction::Down)])
        );
        assert_eq!(live_length.current_size, 25.0);
    }

    #[test]
    fn remote_snake_interpolation_does_not_reuse_stale_tail_for_new_head_tick() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(11), 0.0);

        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                head(Vec2::new(99.0, 99.0), Direction::Left),
                TailPoints::new(VecDeque::from([TailTurn::new(
                    Vec2::new(90.0, 99.0),
                    Direction::Down,
                )])),
                length(25.0),
                confirmed_history_at([
                    (Tick(10), head(Vec2::ZERO, Direction::Right)),
                    (Tick(12), head(Vec2::new(10.0, 0.0), Direction::Right)),
                ]),
                confirmed_history_at([(Tick(10), TailPoints::empty())]),
                confirmed_history_at([(Tick(10), length(40.0)), (Tick(12), length(40.0))]),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        let live_tail = entity.get::<TailPoints>().unwrap();
        let live_length = entity.get::<TailLength>().unwrap();
        assert_vec2_close(live_head.position, Vec2::ZERO);
        assert_eq!(live_head.direction, Direction::Right);
        assert!(live_tail.turns.is_empty());
        assert_eq!(live_length.current_size, 40.0);
    }

    #[test]
    fn remote_snake_interpolation_uses_tail_points_ticks_as_interpolation_window() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(12), 0.0);

        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                SnakeHead::default(),
                TailPoints::empty(),
                TailLength::default(),
                confirmed_history_at([
                    (Tick(8), head(Vec2::ZERO, Direction::Right)),
                    (Tick(13), head(Vec2::new(8.0, 0.0), Direction::Right)),
                ]),
                confirmed_history_at([
                    (Tick(10), TailPoints::empty()),
                    (Tick(14), TailPoints::empty()),
                ]),
                confirmed_history_at([(Tick(8), length(40.0)), (Tick(13), length(40.0))]),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        assert_vec2_close(live_head.position, Vec2::new(4.0, 0.0));
        assert_eq!(live_head.direction, Direction::Right);
    }

    #[test]
    fn remote_snake_interpolation_uses_tail_sample_at_head_end_tick() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(11), 0.0);

        let stale_tail = TailPoints::new(VecDeque::from([TailTurn::new(
            Vec2::new(-5.0, 1.0),
            Direction::Down,
        )]));
        let exact_tail = TailPoints::new(VecDeque::from([TailTurn::new(
            Vec2::new(-5.0, 0.0),
            Direction::Down,
        )]));
        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                SnakeHead::default(),
                TailPoints::empty(),
                TailLength::default(),
                confirmed_history_at([
                    (Tick(10), head(Vec2::ZERO, Direction::Right)),
                    (Tick(12), head(Vec2::new(10.0, 0.0), Direction::Right)),
                ]),
                confirmed_history_at([(Tick(10), stale_tail), (Tick(12), exact_tail)]),
                confirmed_history_at([(Tick(10), length(40.0)), (Tick(12), length(40.0))]),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        let live_tail = entity.get::<TailPoints>().unwrap();
        let live_length = entity.get::<TailLength>().unwrap();
        assert_vec2_close(live_head.position, Vec2::new(5.0, 0.0));
        assert_eq!(
            live_tail.turns,
            VecDeque::from([TailTurn::new(Vec2::new(-5.0, 0.0), Direction::Down)])
        );
        assert!(
            first_diagonal_segment(&live_tail.polyline(live_head, live_length.current_size))
                .is_none()
        );
    }

    #[test]
    fn remote_snake_interpolation_preserves_front_turn_near_end_sample() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, interpolate_remote_snakes);
        insert_timeline(&mut app, Tick(365), 0.9996338);

        let end_tail = TailPoints::new(VecDeque::from([
            TailTurn::new(Vec2::new(-185.24792, 20.859436), Direction::Down),
            TailTurn::new(Vec2::new(-185.24792, -54.538353), Direction::Right),
        ]));
        let snake = app
            .world_mut()
            .spawn((
                Interpolated,
                SnakeHead::default(),
                TailPoints::empty(),
                TailLength::default(),
                confirmed_history_at([
                    (
                        Tick(364),
                        head(Vec2::new(-185.24792, 20.009436), Direction::Up),
                    ),
                    (
                        Tick(366),
                        head(Vec2::new(-184.39792, 20.859436), Direction::Right),
                    ),
                ]),
                confirmed_history_at([
                    (
                        Tick(364),
                        TailPoints::new(VecDeque::from([TailTurn::new(
                            Vec2::new(-185.24792, -54.538353),
                            Direction::Right,
                        )])),
                    ),
                    (Tick(366), end_tail),
                ]),
                confirmed_history_at([(Tick(364), length(160.0)), (Tick(366), length(160.0))]),
            ))
            .id();

        app.update();

        let entity = app.world().entity(snake);
        let live_head = entity.get::<SnakeHead>().unwrap();
        let live_tail = entity.get::<TailPoints>().unwrap();
        let live_length = entity.get::<TailLength>().unwrap();
        assert!(
            first_diagonal_segment(&live_tail.polyline(live_head, live_length.current_size))
                .is_none(),
            "tail should stay axis-aligned: head={live_head:?}, tail={live_tail:?}"
        );
        assert_vec2_close(
            live_tail.turns.front().unwrap().position,
            Vec2::new(-185.24792, 20.859436),
        );
    }
}
