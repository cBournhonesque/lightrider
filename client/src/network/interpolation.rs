//! We disabled per-component interpolation because we need to do:
//! - non-linear interpolation
//! - the interpolation logic uses multiple entities/components
use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::client::*;
use shared::movement::shorten_tail;
use shared::network::protocol::prelude::{Direction, HeadDirection, HeadPoint, TailLength, TailParent, TailPoints};

// TODO: we might not need to do this at all for TailLength, Speed, Acceleration
//  because we only need to interpolate the visual representation of the snake
//  whereas the prediction is for the snake's movement
//  this shows that we want to separate the sync mode between prediction/interpolation

pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, interpolate_snake);
    }
}

pub(crate) fn interpolate_snake(
    mut heads: Query<
        (&mut HeadPoint, &mut HeadDirection, &mut TailLength, &InterpolateStatus<HeadPoint>, &InterpolateStatus<HeadDirection>, &InterpolateStatus<TailLength>)>,
    mut tails: Query<(&TailParent, &mut TailPoints, &InterpolateStatus<TailPoints>)>
) {

    for (parent, mut tail, tail_status) in tails.iter_mut() {
        let (mut head_point, mut head_direction, mut length,  head_point_status, head_direction_status, length_status) = heads.get_mut(parent.0).unwrap();

        let Some((start_tick, tail_start)) = &tail_status.start else {
            continue;
        };
        let Some((_, head_point_start)) = &head_point_status.start else {
            panic!("we should also have a head point start");
        };
        let Some((_, head_direction_start)) = &head_direction_status.start else {
            panic!("we should also have a head direction start");
        };
        let Some((_, length_start)) = &length_status.start else {
            panic!("we should also have a tail length start");
        };
        if tail_status.current == *start_tick {
            *tail = tail_start.clone();
            *head_point = head_point_start.clone();
            *head_direction = head_direction_start.clone();
            *length = length_start.clone();
            continue;
        }

        let Some((end_tick, tail_end)) = &tail_status.end else {
            continue;
        };
        let Some((_, head_point_end)) = &head_point_status.end else {
            panic!("we should also have a head point end");
        };
        let Some((_, head_direction_end)) = &head_direction_status.end else {
            panic!("we should also have a head direction end");
        };
        let Some((_, length_end)) = &length_status.end else {
            panic!("we should also have a tail length end");
        };
        if tail_status.current == *end_tick {
            *tail = tail_end.clone();
            *head_point = head_point_end.clone();
            *head_direction = head_direction_end.clone();
            *length = length_end.clone();
            continue;
        }
        assert_ne!(start_tick, end_tick);

        // we need to interpolate between the two tails. It will be similar to the start tail with some added points
        // at the front, and then we will remove points from the back to respect the length
        *tail = tail_start.clone();
        *head_point = head_point_start.clone();

        // interpolation ratio
        let t = (tail_status.current - *start_tick) as f32
            / (*end_tick - *start_tick) as f32;

        // linear interpolation for the length
        *length = length_start.clone() * (1.0 - t) + length_end.clone() * t;

        // distance between the two heads while remaining on the tail path
        let mut tail_diff_length = 0.0;

        // distance that we need to move the head point while remaining on the tail path
        let mut pos_distance_to_move = 0.0;
        // segment in which the starting pos is (0 is [front_tail -> head])
        let mut segment_idx = 0;

        // 1. we need to find in which end tail segment the start head is, and the difference in length
        //    between the two tails
        for (i, (from, to)) in tail_end.pairs_front_to_back(&(head_point_end.0, head_direction_end.0)).enumerate() {
            // we found the segment on which the head point is
            if shared::utils::geometry::segment_contains_point(&from.0, &to.0, &head_point_start.0) {
                tail_diff_length += to.0.distance(head_point_start.0);
                // if the head point is at a turn point, we need to add a turn point right now before we move the head point
                // in the later stage (only if it's actually turning!)
                if head_point_start.0 == from.0 && head_direction_start.0 != from.1 {
                    tail.0.push_front(from.clone());
                }
                pos_distance_to_move = t * tail_diff_length;
                segment_idx = i;
                break;
            } else {
                tail_diff_length += from.0.distance(to.0);
            }
        }

        // 2. now move the head point by `pos_distance_to_move` while remaining on the end tail path
        length.current_size += pos_distance_to_move;
        for (from, to) in tail_end.pairs_back_to_front(&(head_point_end.0, head_direction_end.0)).skip(tail_end.0.len() - 1 - segment_idx) {
            if pos_distance_to_move < 1000.0 * f32::EPSILON {
                info!("found final segment, on point start?");
                break;
            }
            let dist = head_point.0.distance(to.0);
            // the head tail has to go to the next segment
            if dist < pos_distance_to_move {
                pos_distance_to_move -= dist;
                tail.0.push_front(to.clone());
                head_point.0 = to.0;
            } else {
                // we found the segment on which the head point is
                head_point.0 = from.0 + from.1.delta() * pos_distance_to_move;
                head_direction.0 = from.1;
                break;
            }
        }

        // 3. then shorten the back of the tail
        shorten_tail(&mut tail, &head_point, &mut length);
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
//             if status.current == *start_tick {
//                 *component = start_value;
//                 continue;
//             }
//             if let Some((end_tick, end_value)) = &status.end {
//                 if status.current == *end_tick {
//                     *component = end_value;
//                     continue;
//                 }
//                 if start_tick != end_tick {
//                     let t =
//                         (status.current - *start_tick) as f32 / (*end_tick - *start_tick) as f32;
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
        let head = app.world.spawn( (
                    HeadPoint(Vec2::new(0.0, 0.0)),
                    HeadDirection(Direction::Up),
                    TailLength {
                        current_size: 100.0,
                        target_size: 100.0,
                    },
                    InterpolateStatus::<HeadPoint> {
                        start: Some((Tick(0), HeadPoint(Vec2::new(0.0, 0.0)))),
                        end: Some((Tick(4), HeadPoint(Vec2::new(0.0, 100.0)))),
                        current: Tick(2),
                    },
                    InterpolateStatus::<HeadDirection> {
                        start: Some((Tick(0), HeadDirection(Direction::Up))),
                        end: Some((Tick(4), HeadDirection(Direction::Up))),
                        current: Tick(2),
                    },
                    InterpolateStatus::<TailLength> {
                        start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                        end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                        current: Tick(2),
                    },
        )).id();
        let tail = app.world.spawn((
            TailParent(head),
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, -100.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, 0.0), Direction::Up)]
                )))),
                current: Tick(2),
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(head).get::<HeadPoint>().unwrap().0, Vec2::new(0.0, 50.0));
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [(Vec2::new(0.0, -50.0), Direction::Up)]
        ));
    }

    #[test]
    fn test_interpolate_turn() {
        let mut app = App::new();
        let head = app.world.spawn( (
            HeadPoint(Vec2::new(0.0, 0.0)),
            HeadDirection(Direction::Up),
            TailLength {
                current_size: 100.0,
                target_size: 100.0,
            },
            InterpolateStatus::<HeadPoint> {
                start: Some((Tick(0), HeadPoint(Vec2::new(0.0, 0.0)))),
                end: Some((Tick(4), HeadPoint(Vec2::new(50.0, 50.0)))),
                current: Tick(3),
            },
            InterpolateStatus::<HeadDirection> {
                start: Some((Tick(0), HeadDirection(Direction::Up))),
                end: Some((Tick(4), HeadDirection(Direction::Right))),
                current: Tick(3),
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current: Tick(3),
            },
        )).id();
        let tail = app.world.spawn((
            TailParent(head),
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, -100.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 0.0), Direction::Up)]
                )))),
                current: Tick(3),
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(head).get::<HeadPoint>().unwrap().0, Vec2::new(25.0, 50.0));
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [(Vec2::new(0.0, 50.0), Direction::Right),
                (Vec2::new(0.0, -25.0), Direction::Up)]
        ));
    }

    #[test]
    fn test_interpolate_on_turn_point() {
        let mut app = App::new();
        let head = app.world.spawn( (
            HeadPoint(Vec2::new(0.0, 0.0)),
            HeadDirection(Direction::Up),
            TailLength {
                current_size: 100.0,
                target_size: 100.0,
            },
            InterpolateStatus::<HeadPoint> {
                start: Some((Tick(0), HeadPoint(Vec2::new(0.0, 0.0)))),
                end: Some((Tick(4), HeadPoint(Vec2::new(50.0, 50.0)))),
                current: Tick(2),
            },
            InterpolateStatus::<HeadDirection> {
                start: Some((Tick(0), HeadDirection(Direction::Up))),
                end: Some((Tick(4), HeadDirection(Direction::Right))),
                current: Tick(2),
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current: Tick(2),
            },
        )).id();
        let tail = app.world.spawn((
            TailParent(head),
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, -100.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, 50.0), Direction::Right),
                        (Vec2::new(0.0, 0.0), Direction::Up)]
                )))),
                current: Tick(2),
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(head).get::<HeadPoint>().unwrap().0, Vec2::new(0.0, 50.0));
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [(Vec2::new(0.0, -50.0), Direction::Up)]
        ));
    }

    #[test]
    fn test_interpolate_immediate_turn() {
        let mut app = App::new();
        let head = app.world.spawn( (
            HeadPoint(Vec2::new(0.0, 0.0)),
            HeadDirection(Direction::Up),
            TailLength {
                current_size: 100.0,
                target_size: 100.0,
            },
            InterpolateStatus::<HeadPoint> {
                start: Some((Tick(0), HeadPoint(Vec2::new(0.0, 0.0)))),
                end: Some((Tick(4), HeadPoint(Vec2::new(50.0, 50.0)))),
                current: Tick(2),
            },
            InterpolateStatus::<HeadDirection> {
                start: Some((Tick(0), HeadDirection(Direction::Up))),
                end: Some((Tick(4), HeadDirection(Direction::Right))),
                current: Tick(2),
            },
            InterpolateStatus::<TailLength> {
                start: Some((Tick(0), TailLength { current_size: 100.0, target_size: 100.0 })),
                end: Some((Tick(4), TailLength { current_size: 100.0, target_size: 100.0 })),
                current: Tick(2),
            },
        )).id();
        let tail = app.world.spawn((
            TailParent(head),
            TailPoints(VecDeque::default()),
            InterpolateStatus::<TailPoints> {
                start: Some((Tick(0), TailPoints(VecDeque::from(
                    [(Vec2::new(0.0, -100.0), Direction::Up)]
                )))),
                end: Some((Tick(4), TailPoints(VecDeque::from(
                    [(Vec2::new(50.0, 0.0), Direction::Up),
                        (Vec2::new(0.0, 0.0), Direction::Right)]
                )))),
                current: Tick(2),
            },
        )).id();

        app.add_systems(Update, interpolate_snake);
        app.update();
        assert_eq!(app.world.entity(head).get::<HeadPoint>().unwrap().0, Vec2::new(50.0, 0.0));
        assert_eq!(app.world.entity(tail).get::<TailPoints>().unwrap().0, VecDeque::from(
            [(Vec2::new(0.0, 0.0), Direction::Right),
                (Vec2::new(0.0, -50.0), Direction::Up)]
        ));
    }
}