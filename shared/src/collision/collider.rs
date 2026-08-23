use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::query::Or;
use bevy::prelude::*;
use lightyear::prelude::{ControlledBy, Interpolated, InterpolationDelay, LocalTimeline, Tick};

use crate::config::GameConfig;
use crate::movement::SimulationSet;
use crate::network::protocol::prelude::{
    RoomId, SnakeHead, TailLength, TailPathHistory, TailPoints, TailPolyline,
};
use crate::spatial::{TailSegment, TailSpatialIndex};
use crate::utils::geometry::ray_segment_intersection;
use crate::utils::query::Simulated;

pub struct ColliderPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy, Reflect)]
pub enum ColliderSet {
    // Pre-movement geometry, such as proximity boost detection.
    UpdateColliders,
    // Post-movement collision information, such as death/food checks.
    ComputeCollision,
}

impl Plugin for ColliderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>();
        app.add_message::<SnakeFrictionEvent>();
        app.configure_sets(
            FixedUpdate,
            (
                ColliderSet::UpdateColliders,
                SimulationSet::Movement,
                ColliderSet::ComputeCollision,
            )
                .chain(),
        );
        app.add_systems(
            FixedUpdate,
            snake_friction.in_set(ColliderSet::UpdateColliders),
        );

        app.register_type::<ColliderSet>();
    }
}

#[derive(Message, Debug, PartialEq)]
pub struct SnakeFrictionEvent {
    pub main: Entity,
    pub other: Entity,
    pub distance: f32,
}

pub const MAX_FRICTION_DISTANCE: f32 = 14.0;

#[derive(Clone, Copy)]
struct FrictionTail<'a> {
    tail: &'a TailPolyline,
    length: Option<&'a TailLength>,
    history: Option<&'a TailPathHistory>,
}

/// Friction is computed both on the client and the server because it influences movement.
pub(crate) fn snake_friction(
    // Only predicted/client-local or server-authoritative snakes receive boost events, but
    // interpolated remote tails are still valid obstacles for client-side prediction.
    config: Res<GameConfig>,
    timeline: Option<Res<LocalTimeline>>,
    clients: Query<&InterpolationDelay>,
    boosted: Query<(Entity, &SnakeHead, &RoomId, Option<&ControlledBy>), Simulated>,
    tails: Query<
        (
            Entity,
            &SnakeHead,
            &TailPoints,
            Option<&TailLength>,
            Option<&TailPathHistory>,
            &RoomId,
        ),
        Or<(Simulated, With<Interpolated>)>,
    >,
    mut writer: MessageWriter<SnakeFrictionEvent>,
) {
    let max_distance = config.movement.boost_distance;
    if max_distance <= 0.0 {
        return;
    }
    let lag_compensation_enabled = config.network.lag_compensation.enabled;
    let retained_extra_length = if lag_compensation_enabled {
        crate::movement::lag_compensation_extra_length(&config)
    } else {
        0.0
    };
    let tail_snapshots = tails
        .iter()
        .map(|(entity, head, tail, length, history, room)| {
            let length_value = length
                .map(|length| {
                    length.current_size
                        + if history.is_some() {
                            retained_extra_length
                        } else {
                            0.0
                        }
                })
                .unwrap_or(0.0);
            (
                entity,
                *room,
                tail.polyline(head, length_value).axis_aligned(),
                length,
                history,
            )
        })
        .collect::<Vec<_>>();
    let tail_index = TailSpatialIndex::from_tails(
        tail_snapshots
            .iter()
            .map(|(entity, room, tail, _, _)| (*entity, *room, tail)),
    );
    let tail_lookup = lag_compensation_enabled.then(|| {
        tail_snapshots
            .iter()
            .map(|(entity, _, tail, length, history)| {
                (
                    *entity,
                    FrictionTail {
                        tail,
                        length: *length,
                        history: *history,
                    },
                )
            })
            .collect::<EntityHashMap<_>>()
    });
    let tick = timeline.as_ref().map(|timeline| timeline.tick());
    for (entity, head, room, controlled_by) in boosted.iter() {
        let origin = head.position;
        let direction = head.direction.delta();
        let interpolation_delay = if tail_lookup.is_some() {
            controlled_by.and_then(|controlled_by| clients.get(controlled_by.owner).ok().copied())
        } else {
            None
        };
        let (left_hit, right_hit) = if let Some(tail_lookup) = tail_lookup.as_ref() {
            (
                nearest_lag_compensated_tail_ray_hit(
                    origin,
                    direction.perp(),
                    max_distance,
                    entity,
                    *room,
                    interpolation_delay,
                    tick,
                    &tail_index,
                    tail_lookup,
                ),
                nearest_lag_compensated_tail_ray_hit(
                    origin,
                    -direction.perp(),
                    max_distance,
                    entity,
                    *room,
                    interpolation_delay,
                    tick,
                    &tail_index,
                    tail_lookup,
                ),
            )
        } else {
            (
                nearest_tail_ray_hit(
                    origin,
                    direction.perp(),
                    max_distance,
                    entity,
                    *room,
                    &tail_index,
                ),
                nearest_tail_ray_hit(
                    origin,
                    -direction.perp(),
                    max_distance,
                    entity,
                    *room,
                    &tail_index,
                ),
            )
        };

        if let Some((distance, other)) = nearest_hit(left_hit, right_hit) {
            writer.write(SnakeFrictionEvent {
                main: entity,
                other,
                distance,
            });
        }
    }
}

fn nearest_hit(left: Option<(f32, Entity)>, right: Option<(f32, Entity)>) -> Option<(f32, Entity)> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(hit), None) | (None, Some(hit)) => Some(hit),
        (None, None) => None,
    }
}

fn nearest_tail_ray_hit(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    excluded: Entity,
    room: RoomId,
    tail_index: &TailSpatialIndex,
) -> Option<(f32, Entity)> {
    let candidates = if direction.x.abs() >= direction.y.abs() {
        let end = origin + direction * max_distance;
        tail_index.vertical_segments_near(room, origin.x.min(end.x), origin.x.max(end.x))
    } else {
        let end = origin + direction * max_distance;
        tail_index.horizontal_segments_near(room, origin.y.min(end.y), origin.y.max(end.y))
    };
    nearest_tail_ray_hit_from_segments(origin, direction, max_distance, excluded, candidates)
}

fn nearest_tail_ray_hit_from_segments<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    excluded: Entity,
    candidates: impl IntoIterator<Item = &'a TailSegment>,
) -> Option<(f32, Entity)> {
    let mut nearest: Option<(f32, Entity)> = None;
    for segment in candidates {
        if segment.owner == excluded {
            continue;
        }
        let Some(distance) =
            ray_segment_intersection(origin, direction, max_distance, segment.start, segment.end)
        else {
            continue;
        };
        if nearest.map_or(true, |(nearest_distance, _)| distance < nearest_distance) {
            nearest = Some((distance, segment.owner));
        }
    }
    nearest
}

fn nearest_lag_compensated_tail_ray_hit(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    excluded: Entity,
    room: RoomId,
    interpolation_delay: Option<InterpolationDelay>,
    tick: Option<Tick>,
    tail_index: &TailSpatialIndex,
    tails: &EntityHashMap<FrictionTail<'_>>,
) -> Option<(f32, Entity)> {
    let candidates = if direction.x.abs() >= direction.y.abs() {
        let end = origin + direction * max_distance;
        tail_index.vertical_segments_near(room, origin.x.min(end.x), origin.x.max(end.x))
    } else {
        let end = origin + direction * max_distance;
        tail_index.horizontal_segments_near(room, origin.y.min(end.y), origin.y.max(end.y))
    };
    nearest_lag_compensated_tail_ray_hit_from_segments(
        origin,
        direction,
        max_distance,
        excluded,
        interpolation_delay,
        tick,
        tails,
        candidates,
    )
}

fn nearest_lag_compensated_tail_ray_hit_from_segments<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    excluded: Entity,
    interpolation_delay: Option<InterpolationDelay>,
    tick: Option<Tick>,
    tails: &EntityHashMap<FrictionTail<'_>>,
    candidates: impl IntoIterator<Item = &'a TailSegment>,
) -> Option<(f32, Entity)> {
    let mut nearest: Option<(f32, Entity)> = None;
    for segment in candidates {
        if segment.owner == excluded {
            continue;
        }
        let Some(candidate_tail) = tails.get(&segment.owner) else {
            continue;
        };
        let window = tail_collision_window(
            segment.owner,
            excluded,
            candidate_tail.length,
            candidate_tail.history,
            candidate_tail.tail,
            interpolation_delay,
            tick,
        );
        let Some((segment_start, segment_end)) =
            clipped_segment_for_window(candidate_tail.tail, segment.index, window)
        else {
            continue;
        };
        let Some(distance) =
            ray_segment_intersection(origin, direction, max_distance, segment_start, segment_end)
        else {
            continue;
        };
        if nearest.map_or(true, |(nearest_distance, _)| distance < nearest_distance) {
            nearest = Some((distance, segment.owner));
        }
    }
    nearest
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TailCollisionWindow {
    start: f32,
    end: f32,
}

fn tail_collision_window(
    owner: Entity,
    main: Entity,
    length: Option<&TailLength>,
    history: Option<&TailPathHistory>,
    tail: &TailPolyline,
    interpolation_delay: Option<InterpolationDelay>,
    tick: Option<Tick>,
) -> TailCollisionWindow {
    let current_length = length
        .map(|length| length.current_size)
        .unwrap_or_else(|| tail.total_length())
        .max(0.0);
    if owner == main {
        return TailCollisionWindow {
            start: 0.0,
            end: current_length,
        };
    }

    let Some((history, interpolation_delay, tick)) = history
        .zip(interpolation_delay)
        .zip(tick)
        .map(|((h, d), t)| (h, d, t))
    else {
        return TailCollisionWindow {
            start: 0.0,
            end: current_length,
        };
    };
    let (visual_tick, overstep) = interpolation_delay.tick_and_overstep(tick);
    let Some(sample) = history.sample_at(visual_tick, overstep) else {
        return TailCollisionWindow {
            start: 0.0,
            end: current_length,
        };
    };
    let head_offset = (history.head_distance - sample.head_distance).max(0.0);
    TailCollisionWindow {
        start: head_offset,
        end: head_offset + sample.length.max(0.0),
    }
}

fn clipped_segment_for_window(
    tail: &TailPolyline,
    segment_index: usize,
    window: TailCollisionWindow,
) -> Option<(Vec2, Vec2)> {
    let mut near_distance = 0.0;
    for (index, (far, near)) in tail.pairs_front_to_back().enumerate() {
        let segment_length = far.0.distance(near.0);
        if segment_length <= f32::EPSILON {
            continue;
        }

        let far_distance = near_distance + segment_length;
        if index == segment_index {
            let overlap_start = near_distance.max(window.start);
            let overlap_end = far_distance.min(window.end);
            if overlap_end <= overlap_start + f32::EPSILON {
                return None;
            }

            let direction = (far.0 - near.0) / segment_length;
            let clipped_near = near.0 + direction * (overlap_start - near_distance);
            let clipped_far = near.0 + direction * (overlap_end - near_distance);
            return Some((clipped_far, clipped_near));
        }

        near_distance = far_distance;
    }

    None
}

#[cfg(test)]
fn nearest_tail_ray_hit_bruteforce<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    excluded: Entity,
    room: &RoomId,
    tails: impl IntoIterator<Item = (Entity, &'a TailPolyline, &'a RoomId)>,
) -> Option<(f32, Entity)> {
    let mut nearest: Option<(f32, Entity)> = None;
    for (other_entity, other_tail, other_room) in tails {
        if other_entity == excluded || other_room != room {
            continue;
        }
        for (segment_start, segment_end) in other_tail.pairs_front_to_back() {
            let Some(distance) = ray_segment_intersection(
                origin,
                direction,
                max_distance,
                segment_start.0,
                segment_end.0,
            ) else {
                continue;
            };
            if nearest.map_or(true, |(nearest_distance, _)| distance < nearest_distance) {
                nearest = Some((distance, other_entity));
            }
        }
    }
    nearest
}

#[cfg(test)]
mod tests {
    #![allow(unused_variables)]
    use std::collections::VecDeque;

    use bevy::prelude::*;
    use lightyear::core::time::PositiveTickDelta;
    use lightyear::prelude::{Interpolated, Predicted, Replicated};

    use crate::network::bundle::snake::SnakeBundle;
    use crate::network::protocol::prelude::{Direction, TailTurn};

    use super::*;

    fn run_fixed_update(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
    }

    fn legacy_tail<const N: usize>(
        points: [(Vec2, Direction); N],
    ) -> (SnakeHead, TailPoints, TailLength) {
        let (head, length, tail) = TailPoints::from_legacy_polyline(VecDeque::from(points));
        (head, tail, length)
    }

    fn polyline<const N: usize>(points: [(Vec2, Direction); N]) -> TailPolyline {
        TailPolyline::new(VecDeque::from(points))
    }

    #[test]
    fn indexed_friction_query_matches_bruteforce() {
        let room = RoomId(1);
        let snake1 = Entity::from_bits(1);
        let snake2 = Entity::from_bits(2);
        let other_room_snake = Entity::from_bits(3);
        let tail1 = polyline([
            (Vec2::new(0.0, 0.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]);
        let tail2 = polyline([
            (Vec2::new(8.0, 50.0), Direction::Up),
            (Vec2::new(8.0, -50.0), Direction::Up),
        ]);
        let other_room_tail = polyline([
            (Vec2::new(2.0, 50.0), Direction::Up),
            (Vec2::new(2.0, -50.0), Direction::Up),
        ]);
        let tails = [
            (snake1, tail1, room),
            (snake2, tail2, room),
            (other_room_snake, other_room_tail, RoomId(2)),
        ];
        let index = TailSpatialIndex::from_tails(
            tails
                .iter()
                .map(|(entity, tail, room)| (*entity, *room, tail)),
        );
        let origin = Vec2::ZERO;
        let direction = Vec2::X;
        let brute_force = nearest_tail_ray_hit_bruteforce(
            origin,
            direction,
            20.0,
            snake1,
            &room,
            tails
                .iter()
                .map(|(entity, tail, room)| (*entity, tail, room)),
        );
        let indexed = nearest_tail_ray_hit(origin, direction, 20.0, snake1, room, &index);

        assert_eq!(indexed, brute_force);
        assert_eq!(indexed, Some((8.0, snake2)));
    }

    #[test]
    fn test_normal_friction() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);
        // snake1: vertical, pointing up
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        // snake2: vertical on the left of snake1
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = legacy_tail([
            (Vec2::new(-MAX_FRICTION_DISTANCE / 1.5, 0.0), Direction::Up),
            (
                Vec2::new(-MAX_FRICTION_DISTANCE / 1.5, -100.0),
                Direction::Up,
            ),
        ]);
        app.world_mut().entity_mut(snake2).insert(points2);
        // snake3: vertical on the right of snake1, closer than snake 2
        let snake3 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points3 = legacy_tail([
            (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, 0.0), Direction::Up),
            (
                Vec2::new(MAX_FRICTION_DISTANCE / 2.0, -100.0),
                Direction::Up,
            ),
        ]);
        app.world_mut().entity_mut(snake3).insert(points3);

        run_fixed_update(&mut app);

        let mut result = app
            .world_mut()
            .get_resource_mut::<Messages<SnakeFrictionEvent>>()
            .unwrap()
            .drain()
            .collect::<Vec<_>>();
        result.sort_by(|a, b| a.main.partial_cmp(&b.main).unwrap());
        let mut expected = vec![
            SnakeFrictionEvent {
                main: snake1,
                other: snake3,
                distance: MAX_FRICTION_DISTANCE / 2.0,
            },
            SnakeFrictionEvent {
                main: snake2,
                other: snake1,
                distance: MAX_FRICTION_DISTANCE / 1.5,
            },
            SnakeFrictionEvent {
                main: snake3,
                other: snake1,
                distance: MAX_FRICTION_DISTANCE / 2.0,
            },
        ];
        expected.sort_by(|a, b| a.main.partial_cmp(&b.main).unwrap());

        assert_eq!(result.len(), expected.len());
        for (actual, expected) in result.iter().zip(expected.iter()) {
            assert_eq!(actual.main, expected.main);
            assert_eq!(actual.other, expected.other);
            assert!(
                (actual.distance - expected.distance).abs() <= f32::EPSILON * 8.0,
                "actual distance {} != expected distance {}",
                actual.distance,
                expected.distance
            );
        }
    }

    #[test]
    fn friction_ignores_snakes_in_other_rooms() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);

        let _snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = legacy_tail([
            (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, 0.0), Direction::Up),
            (
                Vec2::new(MAX_FRICTION_DISTANCE / 2.0, -100.0),
                Direction::Up,
            ),
        ]);
        app.world_mut()
            .entity_mut(snake2)
            .insert((points2, RoomId(1)));

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeFrictionEvent>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![]
        );
    }

    #[test]
    fn predicted_snake_boosts_from_interpolated_remote_tail() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);

        let predicted = app
            .world_mut()
            .spawn((SnakeBundle::default(), Predicted))
            .id();
        app.world_mut().entity_mut(predicted).insert(legacy_tail([
            (Vec2::ZERO, Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let remote = app
            .world_mut()
            .spawn((SnakeBundle::default(), Replicated, Interpolated))
            .id();
        app.world_mut().entity_mut(remote).insert(legacy_tail([
            (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, 100.0), Direction::Up),
            (
                Vec2::new(MAX_FRICTION_DISTANCE / 2.0, -100.0),
                Direction::Up,
            ),
        ]));

        run_fixed_update(&mut app);

        let result = app
            .world_mut()
            .get_resource_mut::<Messages<SnakeFrictionEvent>>()
            .unwrap()
            .drain()
            .collect::<Vec<_>>();

        assert_eq!(
            result,
            vec![SnakeFrictionEvent {
                main: predicted,
                other: remote,
                distance: MAX_FRICTION_DISTANCE / 2.0,
            }]
        );
    }

    #[test]
    fn predicted_snake_boosts_from_repaired_interpolated_remote_tail() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);

        let predicted = app
            .world_mut()
            .spawn((
                SnakeBundle {
                    head: SnakeHead {
                        position: Vec2::new(0.0, 5.0),
                        direction: Direction::Up,
                    },
                    tail_length: TailLength {
                        current_size: 10.0,
                        target_size: 10.0,
                    },
                    ..default()
                },
                Predicted,
            ))
            .id();
        let remote = app
            .world_mut()
            .spawn((
                SnakeBundle {
                    head: SnakeHead {
                        position: Vec2::new(10.0, 10.0),
                        direction: Direction::Right,
                    },
                    tail_points: TailPoints::new(VecDeque::from([TailTurn::new(
                        Vec2::new(35.0, 0.0),
                        Direction::Left,
                    )])),
                    tail_length: TailLength {
                        current_size: 80.0,
                        target_size: 80.0,
                    },
                    ..default()
                },
                Replicated,
                Interpolated,
            ))
            .id();

        run_fixed_update(&mut app);

        let result = app
            .world_mut()
            .get_resource_mut::<Messages<SnakeFrictionEvent>>()
            .unwrap()
            .drain()
            .collect::<Vec<_>>();

        assert_eq!(
            result,
            vec![SnakeFrictionEvent {
                main: predicted,
                other: remote,
                distance: 10.0,
            }]
        );
    }

    #[test]
    fn lag_compensated_friction_ignores_retained_segment_outside_visual_window() {
        let room = RoomId(1);
        let victim = Entity::from_bits(1);
        let remote = Entity::from_bits(2);
        let victim_tail = polyline([
            (Vec2::new(0.0, 50.0), Direction::Right),
            (Vec2::new(-10.0, 50.0), Direction::Right),
        ]);
        let victim_length = TailLength {
            current_size: 10.0,
            target_size: 10.0,
        };
        let remote_tail = polyline([
            (Vec2::new(5.0, 100.0), Direction::Up),
            (Vec2::new(5.0, 0.0), Direction::Up),
            (Vec2::new(5.0, -100.0), Direction::Up),
        ]);
        let remote_length = TailLength {
            current_size: 100.0,
            target_size: 100.0,
        };
        let mut remote_history = TailPathHistory::default();
        remote_history.record_sample(Tick(9), 100.0, 4);
        remote_history.advance_head(100.0);
        remote_history.record_sample(Tick(10), 100.0, 4);
        let index = TailSpatialIndex::from_tails([
            (victim, room, &victim_tail),
            (remote, room, &remote_tail),
        ]);
        let tails = EntityHashMap::from_iter([
            (
                victim,
                FrictionTail {
                    tail: &victim_tail,
                    length: Some(&victim_length),
                    history: None,
                },
            ),
            (
                remote,
                FrictionTail {
                    tail: &remote_tail,
                    length: Some(&remote_length),
                    history: Some(&remote_history),
                },
            ),
        ]);
        let delay = InterpolationDelay {
            delay: PositiveTickDelta::lit("1"),
        };

        let hit = nearest_lag_compensated_tail_ray_hit(
            Vec2::new(0.0, 50.0),
            Vec2::X,
            10.0,
            victim,
            room,
            Some(delay),
            Some(Tick(10)),
            &index,
            &tails,
        );

        assert_eq!(hit, None);
    }

    #[test]
    fn lag_compensated_friction_hits_segment_inside_visual_window() {
        let room = RoomId(1);
        let victim = Entity::from_bits(1);
        let remote = Entity::from_bits(2);
        let victim_tail = polyline([
            (Vec2::new(0.0, -50.0), Direction::Right),
            (Vec2::new(-10.0, -50.0), Direction::Right),
        ]);
        let victim_length = TailLength {
            current_size: 10.0,
            target_size: 10.0,
        };
        let remote_tail = polyline([
            (Vec2::new(5.0, 100.0), Direction::Up),
            (Vec2::new(5.0, 0.0), Direction::Up),
            (Vec2::new(5.0, -100.0), Direction::Up),
        ]);
        let remote_length = TailLength {
            current_size: 100.0,
            target_size: 100.0,
        };
        let mut remote_history = TailPathHistory::default();
        remote_history.record_sample(Tick(9), 100.0, 4);
        remote_history.advance_head(100.0);
        remote_history.record_sample(Tick(10), 100.0, 4);
        let index = TailSpatialIndex::from_tails([
            (victim, room, &victim_tail),
            (remote, room, &remote_tail),
        ]);
        let tails = EntityHashMap::from_iter([
            (
                victim,
                FrictionTail {
                    tail: &victim_tail,
                    length: Some(&victim_length),
                    history: None,
                },
            ),
            (
                remote,
                FrictionTail {
                    tail: &remote_tail,
                    length: Some(&remote_length),
                    history: Some(&remote_history),
                },
            ),
        ]);
        let delay = InterpolationDelay {
            delay: PositiveTickDelta::lit("1"),
        };

        let hit = nearest_lag_compensated_tail_ray_hit(
            Vec2::new(0.0, -50.0),
            Vec2::X,
            10.0,
            victim,
            room,
            Some(delay),
            Some(Tick(10)),
            &index,
            &tails,
        );

        assert_eq!(hit, Some((5.0, remote)));
    }
}
