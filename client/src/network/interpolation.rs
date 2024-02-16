//! We disabled per-component interpolation because we need to do:
//! - non-linear interpolation
//! - the interpolation logic uses multiple entities/components
use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::client::*;

use shared::movement::{shorten_tail};
use shared::network::protocol::prelude::{Direction, TailLength, TailPoints};

// TODO: we might not need to do this at all for TailLength, Speed, Acceleration
//  because we only need to interpolate the visual representation of the snake
//  whereas the prediction is for the snake's movement
//  this shows that we want to separate the sync mode between prediction/interpolation

pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, interpolate_snake.in_set(InterpolationSet::Interpolate));
    }
}

pub(crate) fn interpolate_snake(
    mut tails: Query<(&mut TailPoints, Ref<InterpolateStatus<TailPoints>>, &mut TailLength, Ref<InterpolateStatus<TailLength>>)>
) {
    for (mut tail, tail_status, mut length, length_status) in tails.iter_mut() {
        if !tail_status.is_changed() && !length_status.is_changed() {
            // nothing changed, no need to interpolate?
            continue;
        }
        let Some((start_tick, tail_start)) = &tail_status.start else {
            continue;
        };
        let Some((_, length_start)) = &length_status.start else {
            panic!("we should also have a tail length start");
        };
        let end = tail_status.end.as_ref().map(|x| x.0);
        info!(
            current = ?tail_status.current_tick,
            start = ?start_tick,
            end = ?end,
            "interpolate ticks"
        );
        trace!(
            ?tail,
            ?tail_status,
            "interpolate situation"
        );
        let Some((end_tick, tail_end)) = &tail_status.end else {
            continue;
        };
        let Some((_, length_end)) = &length_status.end else {
            panic!("we should also have a tail length end");
        };
        info!(start = ?tail_start.front(), end = ?tail_end.front(), "Updating tail");
        assert_ne!(start_tick, end_tick);

        // we need to interpolate between the two tails. It will be similar to the start tail with some added points
        // at the front, and then we will remove points from the back to respect the length
        *tail = tail_start.clone();

        // interpolation ratio
        let t = tail_status.interpolation_fraction().unwrap();

        // linear interpolation for the length
        *length = length_start.clone() * (1.0 - t) + length_end.clone() * t;

        // distance between the two heads while remaining on the tail path
        let mut tail_diff_length = 0.0;

        // distance that we need to move the head point while remaining on the tail path
        let mut pos_distance_to_move = 0.0;
        // segment in which the starting pos is (0 is [front_tail -> head])
        let mut segment_idx = usize::MAX;

        // 1. we need to find in which end tail segment the start head is, and the difference in length
        //    between the two tails
        for (i, (from, to)) in tail_end.pairs_front_to_back().enumerate() {
            // we found the segment on which the head point is
            if shared::utils::geometry::segment_contains_point(&from.0, &to.0, &tail.front().0) {
                tail_diff_length += to.0.distance(tail.front().0);
                // if the head point is at a turn point, we need to add a turn point right now before we move the head point
                // in the later stage (only if it's actually turning!)
                if tail.front().0 == from.0 && tail.front().1 != from.1 {
                    tail.front_mut().1 = from.1;
                    tail.0.push_front(from.clone());
                }
                pos_distance_to_move = t * tail_diff_length;
                segment_idx = i;
                break;
            } else {
                tail_diff_length += from.0.distance(to.0);
            }
        }
        if pos_distance_to_move == 0.0 {
            continue;
        }
        if segment_idx == usize::MAX {
            // the difference between start/end is bigger than the length of the snake
            panic!("could not find segment on which the head point is");
        }

        // 2. now move the head point by `pos_distance_to_move` while remaining on the end tail path
        length.current_size += pos_distance_to_move;
        for (from, to) in tail_end.pairs_back_to_front().skip(tail_end.0.len() - 2 - segment_idx) {
            let dist = tail.front().0.distance(to.0);
            // the head tail has to go to the next segment
            if dist <= pos_distance_to_move {
                // move the front of the tail to the end of the segment
                tail.front_mut().0 = to.0;
                tail.front_mut().1 = to.1;
                if dist == pos_distance_to_move {
                    // we advanced by the correct amount
                    break;
                } else {
                    // add a new point
                    pos_distance_to_move -= dist;
                    tail.0.push_front(to.clone());
                }
            } else {
                trace!("finished moving head point on the tail path");
                // we found the segment on which the head point is
                tail.front_mut().0 += from.1.delta() * pos_distance_to_move;
                tail.front_mut().1 = from.1;
                break;
            }
        }

        // 3. then shorten the back of the tail
        shorten_tail(&mut tail, &mut length);
    }
}


// pub(crate) fn linear_interpolate<C: Component + Clone>(
//     mut query: Query<(&mut C, &InterpolateStatus<C>)>,
// ) where
//     Components: SyncMetadata<C>,
// {
//     for (mut component, status) in query.iter_mut() {
//         // NOTE: it is possible that we reach start_tick when end_tick is not set
//         if let Some((start_tick, start_value)) = &status.start {
//             if status.current_tick == *start_tick {
//                 *component = start_value;
//                 continue;
//             }
//             if let Some((end_tick, end_value)) = &status.end {
//                 if status.current_tick == *end_tick {
//                     *component = end_value;
//                     continue;
//                 }
//                 if start_tick != end_tick {
//                     let t =
//                         (status.current_tick - *start_tick) as f32 / (*end_tick - *start_tick) as f32;
//                     let value = LinearInterpolator::lerp(start_value, end_value, t);
//                     *component = value;
//                 } else {
//                     *component = start_value;
//                 }
//             }
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    #[test]
    fn test_interpolate_single_segment() {
        let mut app = App::new();
        let tail = app.world.spawn((
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, 0.0), Direction::Up),
                        (Vec2::new(0.0, -100.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, 20.0), Direction::Up),
                        (Vec2::new(0.0, -80.0), Direction::Up)]
                )))),
                current_tick: Tick(2),
                current_overstep: 0.0,
            },
            TailLength {
                current_size: 100.0,
                target_size: 100.0,
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current_tick: Tick(2),
                current_overstep: 0.0,
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [(Vec2::new(0.0, 10.0), Direction::Up),
                (Vec2::new(0.0, -90.0), Direction::Up)]
        ));
    }

    #[test]
    fn test_interpolate_turn_big_move() {
        let mut app = App::new();
        let tail = app.world.spawn((
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [
                        (Vec2::new(0.0, 0.0), Direction::Up),
                        (Vec2::new(0.0, -120.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [(Vec2::new(50.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, -20.0), Direction::Up)]
                )))),
                current_tick: Tick(3),
                current_overstep: 0.0,
            },
            TailLength {
                current_size: 120.0,
                target_size: 120.0,
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current_tick: Tick(3),
                current_overstep: 0.0,
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [
                (Vec2::new(25.0, 50.0), Direction::Right),
                (Vec2::new(0.0, 50.0), Direction::Right),
                (Vec2::new(0.0, -45.0), Direction::Up)]
        ));
    }

    #[test]
    fn test_interpolate_turn_small_move() {
        let mut app = App::new();
        let tail = app.world.spawn((
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [
                        (Vec2::new(40.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, -10.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [
                        (Vec2::new(50.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 0.0), Direction::Up)]
                )))),
                current_tick: Tick(3),
                current_overstep: 0.0,
            },
            TailLength {
                current_size: 100.0,
                target_size: 100.0,
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current_tick: Tick(3),
                current_overstep: 0.0,
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [
                (Vec2::new(47.5, 50.0), Direction::Right),
                (Vec2::new(0.0, 50.0), Direction::Right),
                (Vec2::new(0.0, -2.5), Direction::Up)]
        ));
    }

    #[test]
    fn test_interpolate_on_turn_point() {
        let mut app = App::new();
        let tail = app.world.spawn((
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, 0.0), Direction::Up),
                        (Vec2::new(0.0, -100.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [
                        (Vec2::new(50.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 0.0), Direction::Up)]
                )))),
                current_tick: Tick(2),
                current_overstep: 0.0,
            },
            TailLength {
                current_size: 100.0,
                target_size: 100.0,
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current_tick: Tick(2),
                current_overstep: 0.0,
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [
                (Vec2::new(0.0, 50.0), Direction::Right),
                (Vec2::new(0.0, -50.0), Direction::Up)]
        ));
    }

    #[test]
    fn test_interpolate_immediate_turn() {
        let mut app = App::new();
        let tail = app.world.spawn((
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [
                        (Vec2::new(0.0, 0.0), Direction::Up),
                        (Vec2::new(0.0, -100.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [
                        (Vec2::new(50.0, 50.0), Direction::Right),
                        (Vec2::new(50.0, 0.0), Direction::Up),
                        (Vec2::new(0.0, 0.0), Direction::Right)]
                )))),
                current_tick: Tick(2),
                current_overstep: 0.0,
            },
            TailLength {
                current_size: 100.0,
                target_size: 100.0,
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current_tick: Tick(2),
                current_overstep: 0.0,
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [
                (Vec2::new(50.0, 0.0), Direction::Up),
                (Vec2::new(0.0, 0.0), Direction::Right),
                (Vec2::new(0.0, -50.0), Direction::Up)]
        ));
    }
}