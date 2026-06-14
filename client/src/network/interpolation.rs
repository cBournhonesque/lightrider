use std::collections::VecDeque;

use bevy::prelude::*;
use lightyear::frame_interpolation::{FrameInterpolate, FrameInterpolationPlugin};
use lightyear::prelude::*;

use shared::network::protocol::prelude::*;

const GEOMETRY_EPSILON: f32 = 0.001;
const INTERPOLATION_PATH_HINT: f32 = 100_000.0;

pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameInterpolationPlugin::<SnakeHead>::default());
        app.add_plugins(FrameInterpolationPlugin::<TailLength>::default());
        app.add_systems(
            Update,
            (
                add_frame_interpolation_to_predicted_snakes,
                interpolate_remote_snakes
                    .after(InterpolationSystems::Prepare)
                    .before(InterpolationSystems::Interpolate),
            ),
        );
    }
}

fn add_frame_interpolation_to_predicted_snakes(
    mut commands: Commands,
    heads: Query<
        Entity,
        (
            With<Predicted>,
            With<SnakeHead>,
            Without<FrameInterpolate<SnakeHead>>,
        ),
    >,
    lengths: Query<
        Entity,
        (
            With<Predicted>,
            With<TailLength>,
            Without<FrameInterpolate<TailLength>>,
        ),
    >,
) {
    for snake in &heads {
        commands
            .entity(snake)
            .insert(FrameInterpolate::<SnakeHead>::default());
    }
    for snake in &lengths {
        commands
            .entity(snake)
            .insert(FrameInterpolate::<TailLength>::default());
    }
}

#[derive(Clone, Debug)]
struct HistorySample<C> {
    end_tick: Tick,
    start: C,
    end: C,
    fraction: f32,
}

fn interpolate_remote_snakes(
    timeline: Single<&InterpolationTimeline, With<IsSynced<InterpolationTimeline>>>,
    mut snakes: Query<
        (
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
    let interpolation_tick = timeline.tick();
    let interpolation_overstep = timeline.overstep().to_f32();

    for (
        mut live_head,
        mut live_tail,
        mut live_length,
        head_history,
        tail_history,
        length_history,
    ) in &mut snakes
    {
        let Some(head_sample) =
            sample_history_pair(head_history, interpolation_tick, interpolation_overstep)
        else {
            continue;
        };
        let Some(length_sample) =
            sample_history_pair(length_history, interpolation_tick, interpolation_overstep)
        else {
            continue;
        };
        let Some(end_tail) = tail_history
            .get_present(head_sample.end_tick)
            .or_else(|| tail_history.get_present(interpolation_tick))
            .or_else(|| tail_history.newest_present().map(|(_, tail)| tail))
            .cloned()
        else {
            continue;
        };

        let interpolated_length = interpolate_tail_length(
            length_sample.start,
            length_sample.end,
            length_sample.fraction,
        );
        let (interpolated_head, interpolated_tail) = interpolate_snake_on_confirmed_path(
            &head_sample.start,
            &head_sample.end,
            &end_tail,
            &interpolated_length,
            head_sample.fraction,
        );

        *live_head = interpolated_head;
        *live_tail = interpolated_tail;
        *live_length = interpolated_length;
    }
}

fn sample_history_pair<C: Clone>(
    history: &ConfirmedHistory<C>,
    interpolation_tick: Tick,
    interpolation_overstep: f32,
) -> Option<HistorySample<C>> {
    let start_index = (0..history.len())
        .take_while(|index| {
            history
                .get_nth_tick(*index)
                .is_some_and(|tick| tick <= interpolation_tick)
        })
        .last()?;

    let (start_tick, ConfirmedState::Confirmed(start)) = history.get_nth_state(start_index)? else {
        return None;
    };
    let Some((end_tick, ConfirmedState::Confirmed(end))) = history.get_nth_state(start_index + 1)
    else {
        return Some(HistorySample {
            end_tick: start_tick,
            start: start.clone(),
            end: start.clone(),
            fraction: 0.0,
        });
    };

    let fraction = if end_tick == start_tick {
        1.0
    } else {
        (((interpolation_tick - start_tick) as f32 + interpolation_overstep)
            / (end_tick - start_tick) as f32)
            .clamp(0.0, 1.0)
    };

    Some(HistorySample {
        end_tick,
        start: start.clone(),
        end: end.clone(),
        fraction,
    })
}

fn interpolate_snake_on_confirmed_path(
    start: &SnakeHead,
    end: &SnakeHead,
    end_tail: &TailPoints,
    length: &TailLength,
    fraction: f32,
) -> (SnakeHead, TailPoints) {
    let final_polyline = end_tail.polyline(
        end,
        length
            .current_size
            .max(length.target_size)
            .max(INTERPOLATION_PATH_HINT),
    );
    let head_path = head_path_from_final_polyline(start, end, &final_polyline);
    let interpolated_head = walk_path(&head_path, fraction, *end);
    let interpolated_tail = tail_points_behind_head(&final_polyline, &interpolated_head);

    (interpolated_head, interpolated_tail)
}

fn head_path_from_final_polyline(
    start: &SnakeHead,
    end: &SnakeHead,
    final_polyline: &TailPolyline,
) -> Vec<Vec2> {
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
        return path;
    }

    fallback_axis_path(start, end)
}

fn fallback_axis_path(start: &SnakeHead, end: &SnakeHead) -> Vec<Vec2> {
    if (start.position.x - end.position.x).abs() <= GEOMETRY_EPSILON
        || (start.position.y - end.position.y).abs() <= GEOMETRY_EPSILON
    {
        return vec![start.position, end.position];
    }

    let corner = match start.direction {
        Direction::Left | Direction::Right => Vec2::new(end.position.x, start.position.y),
        Direction::Up | Direction::Down => Vec2::new(start.position.x, end.position.y),
    };
    let mut path = vec![start.position];
    if !same_position(start.position, corner) && !same_position(corner, end.position) {
        path.push(corner);
    }
    path.push(end.position);
    path
}

fn walk_path(path: &[Vec2], fraction: f32, end: SnakeHead) -> SnakeHead {
    if path.len() < 2 {
        return end;
    }
    let total_length = path_length(path);
    if total_length <= GEOMETRY_EPSILON {
        return end;
    }

    let mut remaining = total_length * fraction.clamp(0.0, 1.0);
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
) -> TailPoints {
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
        return tail_points_from_tailward_positions(interpolated_head, &tail_positions);
    }

    TailPoints::empty()
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
}
