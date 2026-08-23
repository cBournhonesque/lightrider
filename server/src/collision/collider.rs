//! We compute collisions causing death only on the server.
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use lightyear::prelude::{ControlledBy, InterpolationDelay, LocalTimeline, Tick};
use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{
    DeathReason, RoomId, SnakeCollision, SnakeHead, Speed, TailLength, TailPathHistory, TailPoints,
    TailPolyline,
};
use shared::spatial::{TailSegment, TailSpatialIndex};
use shared::utils::geometry::ray_segment_intersection;
use tracing::{debug, trace};

pub struct ColliderPlugin;

impl Plugin for ColliderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>();
        app.add_message::<SnakeCollision>();
        app.add_systems(
            FixedUpdate,
            (snake_collisions, boundary_collisions).in_set(ColliderSet::ComputeCollision),
        );
    }
}

// Collision of 1 pixel.
pub const COLLISION_DISTANCE: f32 = 1.0;
const COLLISION_RAY_EPSILON: f32 = COLLISION_DISTANCE / 1000.0;

#[derive(Clone, Copy)]
struct CollisionTail<'a> {
    tail: &'a TailPolyline,
    length: &'a TailLength,
    history: Option<&'a TailPathHistory>,
}

pub(crate) fn snake_collisions(
    config: Res<GameConfig>,
    timeline: Option<Res<LocalTimeline>>,
    tails: Query<(
        Entity,
        &SnakeHead,
        &TailPoints,
        &RoomId,
        &Speed,
        &TailLength,
        Option<&TailPathHistory>,
        Option<&ControlledBy>,
    )>,
    clients: Query<&InterpolationDelay>,
    mut writer: MessageWriter<SnakeCollision>,
) {
    let lag_compensation_enabled = config.network.lag_compensation.enabled;
    let retained_extra_length = if lag_compensation_enabled {
        shared::movement::lag_compensation_extra_length(&config)
    } else {
        0.0
    };
    let tail_snapshots = tails
        .iter()
        .map(|(entity, head, tail, room, _, length, history, _)| {
            let length_value = length.current_size
                + if history.is_some() {
                    retained_extra_length
                } else {
                    0.0
                };
            (
                entity,
                *room,
                tail.polyline(head, length_value),
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
                    CollisionTail {
                        tail,
                        length: *length,
                        history: *history,
                    },
                )
            })
            .collect::<EntityHashMap<_>>()
    });
    let tick = timeline.as_ref().map(|timeline| timeline.tick());
    for (entity, head, _tail, room, speed, _length, _history, controlled_by) in tails.iter() {
        let direction = head.direction.delta();
        let sweep_distance = collision_sweep_distance(speed.0);
        if sweep_distance <= 0.0 {
            continue;
        }
        let origin =
            head.position - direction * speed.0.max(0.0) + direction * COLLISION_RAY_EPSILON;
        trace!(head = ?head.position, direction = ?head.direction, "Collision ray cast");
        let interpolation_delay = if tail_lookup.is_some() {
            controlled_by.and_then(|controlled_by| clients.get(controlled_by.owner).ok().copied())
        } else {
            None
        };
        let hit = if let Some(tail_lookup) = tail_lookup.as_ref() {
            nearest_lag_compensated_collision(
                origin,
                direction,
                sweep_distance,
                entity,
                *room,
                interpolation_delay,
                tick,
                &tail_index,
                tail_lookup,
            )
        } else {
            nearest_collision(
                origin,
                direction,
                sweep_distance,
                entity,
                *room,
                &tail_index,
            )
        };
        if let Some(hit) = hit {
            let killer = hit.entity;
            let reason = if killer == entity {
                DeathReason::Suicide
            } else {
                DeathReason::Collision
            };
            debug!(?entity, ?killer, "Collision");
            trace!(
                target: "lightyear_debug::manual",
                kind = "server_snake_collision",
                schedule = "FixedUpdate",
                sample_point = "FixedUpdate",
                tick_id = timeline
                    .as_ref()
                    .map(|timeline| u64::from(timeline.tick().0))
                    .unwrap_or(u64::MAX),
                killed = ?entity,
                killer = ?killer,
                reason = ?reason,
                head_x = head.position.x,
                head_y = head.position.y,
                direction_x = direction.x,
                direction_y = direction.y,
                origin_x = origin.x,
                origin_y = origin.y,
                sweep_distance = sweep_distance,
                hit_distance = hit.distance,
                interpolation_delay_ticks = interpolation_delay
                    .map(|delay| delay.delay.tick_diff())
                    .unwrap_or(0),
                interpolation_delay_overstep = interpolation_delay
                    .map(|delay| delay.delay.overstep().to_f32())
                    .unwrap_or(0.0),
                segment_index = hit.segment_index as u64,
                segment_start_x = hit.segment_start.x,
                segment_start_y = hit.segment_start.y,
                segment_end_x = hit.segment_end.x,
                segment_end_y = hit.segment_end.y,
                "server snake collision"
            );
            writer.write(SnakeCollision {
                killed: entity,
                killer,
                reason,
            });
        }
    }
}

pub(crate) fn boundary_collisions(
    config: Res<GameConfig>,
    tails: Query<(Entity, &SnakeHead)>,
    mut writer: MessageWriter<SnakeCollision>,
) {
    for (entity, head) in &tails {
        if !arena_contains(head.position, config.arena.width, config.arena.height) {
            writer.write(SnakeCollision {
                killed: entity,
                killer: entity,
                reason: DeathReason::Boundary,
            });
        }
    }
}

pub(crate) fn arena_contains(position: Vec2, width: f32, height: f32) -> bool {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    position.x >= -half_width
        && position.x <= half_width
        && position.y >= -half_height
        && position.y <= half_height
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CollisionHit {
    entity: Entity,
    distance: f32,
    segment_index: usize,
    segment_start: Vec2,
    segment_end: Vec2,
}

fn nearest_collision(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    main: Entity,
    room: RoomId,
    tail_index: &TailSpatialIndex,
) -> Option<CollisionHit> {
    let end = origin + direction * max_distance;
    let candidates = if direction.x.abs() >= direction.y.abs() {
        tail_index.vertical_segments_near(room, origin.x.min(end.x), origin.x.max(end.x))
    } else {
        tail_index.horizontal_segments_near(room, origin.y.min(end.y), origin.y.max(end.y))
    };
    nearest_collision_from_segments(origin, direction, max_distance, main, candidates)
}

fn nearest_collision_from_segments<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    main: Entity,
    candidates: impl IntoIterator<Item = &'a TailSegment>,
) -> Option<CollisionHit> {
    let mut nearest: Option<CollisionHit> = None;
    for segment in candidates {
        if segment.owner == main && segment.index == 0 {
            continue;
        }
        let Some(distance) =
            ray_segment_intersection(origin, direction, max_distance, segment.start, segment.end)
        else {
            continue;
        };

        if segment.owner == main && distance <= COLLISION_RAY_EPSILON {
            continue;
        }
        if nearest.map_or(true, |nearest| distance < nearest.distance) {
            nearest = Some(CollisionHit {
                entity: segment.owner,
                distance,
                segment_index: segment.index,
                segment_start: segment.start,
                segment_end: segment.end,
            });
        }
    }
    nearest
}

fn nearest_lag_compensated_collision(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    main: Entity,
    room: RoomId,
    interpolation_delay: Option<InterpolationDelay>,
    tick: Option<Tick>,
    tail_index: &TailSpatialIndex,
    tails: &EntityHashMap<CollisionTail<'_>>,
) -> Option<CollisionHit> {
    let end = origin + direction * max_distance;
    let candidates = if direction.x.abs() >= direction.y.abs() {
        tail_index.vertical_segments_near(room, origin.x.min(end.x), origin.x.max(end.x))
    } else {
        tail_index.horizontal_segments_near(room, origin.y.min(end.y), origin.y.max(end.y))
    };
    nearest_lag_compensated_collision_from_segments(
        origin,
        direction,
        max_distance,
        main,
        interpolation_delay,
        tick,
        tails,
        candidates,
    )
}

fn nearest_lag_compensated_collision_from_segments<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    main: Entity,
    interpolation_delay: Option<InterpolationDelay>,
    tick: Option<Tick>,
    tails: &EntityHashMap<CollisionTail<'_>>,
    candidates: impl IntoIterator<Item = &'a TailSegment>,
) -> Option<CollisionHit> {
    let mut nearest: Option<CollisionHit> = None;
    for segment in candidates {
        if segment.owner == main && segment.index == 0 {
            continue;
        }
        let Some(candidate_tail) = tails.get(&segment.owner) else {
            continue;
        };
        let window = tail_collision_window(
            segment.owner,
            main,
            candidate_tail.length,
            candidate_tail.history,
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

        if segment.owner == main && distance <= COLLISION_RAY_EPSILON {
            continue;
        }
        if nearest.map_or(true, |nearest| distance < nearest.distance) {
            nearest = Some(CollisionHit {
                entity: segment.owner,
                distance,
                segment_index: segment.index,
                segment_start,
                segment_end,
            });
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
    length: &TailLength,
    history: Option<&TailPathHistory>,
    interpolation_delay: Option<InterpolationDelay>,
    tick: Option<Tick>,
) -> TailCollisionWindow {
    if owner == main {
        return TailCollisionWindow {
            start: 0.0,
            end: length.current_size.max(0.0),
        };
    }

    let Some((history, interpolation_delay, tick)) = history
        .zip(interpolation_delay)
        .zip(tick)
        .map(|((h, d), t)| (h, d, t))
    else {
        return TailCollisionWindow {
            start: 0.0,
            end: length.current_size.max(0.0),
        };
    };
    let (visual_tick, overstep) = interpolation_delay.tick_and_overstep(tick);
    let Some(sample) = history.sample_at(visual_tick, overstep) else {
        return TailCollisionWindow {
            start: 0.0,
            end: length.current_size.max(0.0),
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
fn nearest_collision_bruteforce<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    main: Entity,
    room: &RoomId,
    tails: impl IntoIterator<Item = (Entity, &'a TailPolyline, &'a RoomId, &'a Speed)>,
) -> Option<CollisionHit> {
    let mut nearest: Option<CollisionHit> = None;
    for (other_entity, other_tail, other_room, _) in tails {
        if other_room != room {
            continue;
        }
        for (index, (segment_start, segment_end)) in other_tail.pairs_front_to_back().enumerate() {
            if other_entity == main && index == 0 {
                continue;
            }
            let Some(distance) = ray_segment_intersection(
                origin,
                direction,
                max_distance,
                segment_start.0,
                segment_end.0,
            ) else {
                continue;
            };

            if other_entity == main && distance <= COLLISION_RAY_EPSILON {
                continue;
            }
            if nearest.map_or(true, |nearest| distance < nearest.distance) {
                nearest = Some(CollisionHit {
                    entity: other_entity,
                    distance,
                    segment_index: index,
                    segment_start: segment_start.0,
                    segment_end: segment_end.0,
                });
            }
        }
    }
    nearest
}

fn collision_sweep_distance(speed: f32) -> f32 {
    (speed.max(0.0) - COLLISION_RAY_EPSILON).max(0.0)
}

#[cfg(test)]
mod tests {
    #![allow(unused_variables)]
    use std::collections::VecDeque;

    use bevy::prelude::*;
    use lightyear::core::time::PositiveTickDelta;
    use shared::network::bundle::snake::SnakeBundle;
    use shared::network::protocol::prelude::Direction;

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
    fn indexed_collision_query_matches_bruteforce() {
        let room = RoomId(1);
        let snake1 = Entity::from_bits(1);
        let snake2 = Entity::from_bits(2);
        let other_room_snake = Entity::from_bits(3);
        let tail1 = polyline([
            (Vec2::new(0.0, 0.0), Direction::Right),
            (Vec2::new(-100.0, 0.0), Direction::Right),
        ]);
        let tail2 = polyline([
            (Vec2::new(5.0, 50.0), Direction::Up),
            (Vec2::new(5.0, -50.0), Direction::Up),
        ]);
        let other_room_tail = polyline([
            (Vec2::new(2.0, 50.0), Direction::Up),
            (Vec2::new(2.0, -50.0), Direction::Up),
        ]);
        let speed = Speed(10.0);
        let tails = [
            (snake1, tail1, room, speed.clone()),
            (snake2, tail2, room, speed.clone()),
            (other_room_snake, other_room_tail, RoomId(2), speed),
        ];
        let index = TailSpatialIndex::from_tails(
            tails
                .iter()
                .map(|(entity, tail, room, _)| (*entity, *room, tail)),
        );
        let origin = Vec2::ZERO;
        let direction = Vec2::X;
        let brute_force = nearest_collision_bruteforce(
            origin,
            direction,
            10.0,
            snake1,
            &room,
            tails
                .iter()
                .map(|(entity, tail, room, speed)| (*entity, tail, room, speed)),
        );
        let indexed = nearest_collision(origin, direction, 10.0, snake1, room, &index);

        assert_eq!(indexed, brute_force);
        assert_eq!(
            indexed,
            Some(CollisionHit {
                entity: snake2,
                distance: 5.0,
                segment_index: 0,
                segment_start: Vec2::new(5.0, -50.0),
                segment_end: Vec2::new(5.0, 50.0),
            })
        );
    }

    #[test]
    fn test_normal_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert(legacy_tail([
            (Vec2::new(0.0, 1.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        // snake2: horizontal across the snake1 movement sweep
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = legacy_tail([
            (Vec2::new(50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(-50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]);
        app.world_mut().entity_mut(snake2).insert(points2);

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake1,
                killer: snake2,
                reason: DeathReason::Collision,
            }]
        );
    }

    #[test]
    fn test_edges_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert(legacy_tail([
            (Vec2::new(0.0, 1.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = legacy_tail([
            (Vec2::new(100.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(0.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]);
        app.world_mut().entity_mut(snake2).insert(points2);

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake1,
                killer: snake2,
                reason: DeathReason::Collision,
            }]
        );
    }

    #[test]
    fn test_parallel_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);
        // snake1: [0, -100] -> [0, 0]
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = legacy_tail([
            (Vec2::new(0.0, 50.0), Direction::Up),
            (Vec2::new(0.0, -50.0), Direction::Up),
        ]);
        app.world_mut().entity_mut(snake2).insert(points2);

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![]
        );
    }

    #[test]
    fn test_no_collision() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);
        // snake1: [0, -100] -> [0, 0]
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = legacy_tail([
            (Vec2::new(100.0, 10.0), Direction::Right),
            (Vec2::new(0.0, 10.0), Direction::Right),
        ]);
        app.world_mut().entity_mut(snake2).insert(points2);
        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![]
        );
    }

    #[test]
    fn near_miss_inside_visual_glow_does_not_collide() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);

        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert((
            Speed(4.0),
            legacy_tail([
                (Vec2::new(0.0, 4.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ]),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake2).insert(legacy_tail([
            (Vec2::new(100.0, 2.0), Direction::Right),
            (Vec2::new(6.0, 2.0), Direction::Right),
        ]));

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![]
        );
    }

    #[test]
    fn slow_turn_does_not_collide_with_own_corner() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);

        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake).insert((
            Speed(0.85),
            legacy_tail([
                (Vec2::new(0.0, 0.85), Direction::Up),
                (Vec2::ZERO, Direction::Up),
                (Vec2::new(-100.0, 0.0), Direction::Right),
            ]),
        ));

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![]
        );
    }

    #[test]
    fn slow_snake_still_sweeps_actual_movement_distance() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);

        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert((
            Speed(0.85),
            legacy_tail([
                (Vec2::new(0.0, 0.85), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ]),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake2).insert(legacy_tail([
            (Vec2::new(50.0, 0.4), Direction::Right),
            (Vec2::new(-50.0, 0.4), Direction::Right),
        ]));

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake1,
                killer: snake2,
                reason: DeathReason::Collision,
            }]
        );
    }

    #[test]
    fn test_self_collision() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);
        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        let points = legacy_tail([
            (Vec2::new(-COLLISION_DISTANCE / 2.0, 50.0), Direction::Left),
            (Vec2::new(10.0, 50.0), Direction::Left),
            (Vec2::new(10.0, 100.0), Direction::Down),
            (Vec2::new(0.0, 100.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]);
        app.world_mut().entity_mut(snake).insert(points);
        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake,
                killer: snake,
                reason: DeathReason::Suicide,
            }]
        );
    }

    #[test]
    fn test_boundary_collision() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(GameConfig {
            arena: shared::config::ArenaConfig {
                width: 100.0,
                height: 100.0,
            },
            ..default()
        });
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);

        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake).insert(legacy_tail([
            (Vec2::new(60.0, 0.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Right),
        ]));

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake,
                killer: snake,
                reason: DeathReason::Boundary,
            }]
        );
    }

    #[test]
    fn fast_snake_sweeps_between_previous_and_current_head() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);

        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert((
            Speed(4.0),
            legacy_tail([
                (Vec2::new(0.0, 4.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ]),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake2).insert(legacy_tail([
            (Vec2::new(50.0, 2.0), Direction::Right),
            (Vec2::new(-50.0, 2.0), Direction::Right),
        ]));

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake1,
                killer: snake2,
                reason: DeathReason::Collision,
            }]
        );
    }

    #[test]
    fn test_collision_ignores_other_rooms() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);

        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert(legacy_tail([
            (Vec2::new(0.0, 1.0), Direction::Up),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = legacy_tail([
            (Vec2::new(50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(-50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]);
        app.world_mut()
            .entity_mut(snake2)
            .insert((points2, RoomId(1)));

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<SnakeCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![]
        );
        assert_ne!(
            app.world().entity(snake1).get::<RoomId>(),
            app.world().entity(snake2).get::<RoomId>()
        );
    }

    #[test]
    fn lag_compensated_collision_ignores_retained_segment_outside_visual_window() {
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
                CollisionTail {
                    tail: &victim_tail,
                    length: &victim_length,
                    history: None,
                },
            ),
            (
                remote,
                CollisionTail {
                    tail: &remote_tail,
                    length: &remote_length,
                    history: Some(&remote_history),
                },
            ),
        ]);
        let delay = InterpolationDelay {
            delay: PositiveTickDelta::lit("1"),
        };

        let hit = nearest_lag_compensated_collision(
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
    fn lag_compensated_collision_hits_segment_inside_visual_window() {
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
                CollisionTail {
                    tail: &victim_tail,
                    length: &victim_length,
                    history: None,
                },
            ),
            (
                remote,
                CollisionTail {
                    tail: &remote_tail,
                    length: &remote_length,
                    history: Some(&remote_history),
                },
            ),
        ]);
        let delay = InterpolationDelay {
            delay: PositiveTickDelta::lit("1"),
        };

        let hit = nearest_lag_compensated_collision(
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

        assert_eq!(
            hit,
            Some(CollisionHit {
                entity: remote,
                distance: 5.0,
                segment_index: 1,
                segment_start: Vec2::new(5.0, -100.0),
                segment_end: Vec2::new(5.0, 0.0),
            })
        );
    }
}
