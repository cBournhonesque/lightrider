//! We compute collisions causing death only on the server.
use bevy::prelude::*;
use lightyear::prelude::LocalTimeline;
use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{DeathReason, RoomId, SnakeCollision, Speed, TailPoints};
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

pub(crate) fn snake_collisions(
    timeline: Option<Res<LocalTimeline>>,
    tails: Query<(Entity, &TailPoints, &RoomId, &Speed)>,
    mut writer: MessageWriter<SnakeCollision>,
) {
    let tail_index = TailSpatialIndex::from_tails(
        tails
            .iter()
            .map(|(entity, tail, room, _)| (entity, *room, tail)),
    );
    for (entity, tail, room, speed) in tails.iter() {
        let direction = tail.front().1.delta();
        let sweep_distance = collision_sweep_distance(speed.0);
        if sweep_distance <= 0.0 {
            continue;
        }
        let origin =
            tail.front().0 - direction * speed.0.max(0.0) + direction * COLLISION_RAY_EPSILON;
        trace!(head = ?tail.front().0, direction = ?tail.front().1, "Collision ray cast");
        if let Some(hit) = nearest_collision(
            origin,
            direction,
            sweep_distance,
            entity,
            *room,
            &tail_index,
        ) {
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
                head_x = tail.front().0.x,
                head_y = tail.front().0.y,
                direction_x = direction.x,
                direction_y = direction.y,
                origin_x = origin.x,
                origin_y = origin.y,
                sweep_distance = sweep_distance,
                hit_distance = hit.distance,
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
    tails: Query<(Entity, &TailPoints)>,
    mut writer: MessageWriter<SnakeCollision>,
) {
    for (entity, tail) in &tails {
        if !arena_contains(tail.front().0, config.arena.width, config.arena.height) {
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

#[cfg(test)]
fn nearest_collision_bruteforce<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    main: Entity,
    room: &RoomId,
    tails: impl IntoIterator<Item = (Entity, &'a TailPoints, &'a RoomId, &'a Speed)>,
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
    use shared::network::bundle::snake::SnakeBundle;
    use shared::network::protocol::prelude::Direction;

    use super::*;

    fn run_fixed_update(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
    }

    #[test]
    fn indexed_collision_query_matches_bruteforce() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let room = RoomId(1);
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert((
            room,
            Speed(10.0),
            TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 0.0), Direction::Right),
                (Vec2::new(-100.0, 0.0), Direction::Right),
            ])),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake2).insert((
            room,
            Speed(10.0),
            TailPoints::new(VecDeque::from([
                (Vec2::new(5.0, 50.0), Direction::Up),
                (Vec2::new(5.0, -50.0), Direction::Up),
            ])),
        ));
        let other_room_snake = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(other_room_snake).insert((
            RoomId(2),
            Speed(10.0),
            TailPoints::new(VecDeque::from([
                (Vec2::new(2.0, 50.0), Direction::Up),
                (Vec2::new(2.0, -50.0), Direction::Up),
            ])),
        ));

        let mut query = app
            .world_mut()
            .query::<(Entity, &TailPoints, &RoomId, &Speed)>();
        let index = TailSpatialIndex::from_tails(
            query
                .iter(app.world())
                .map(|(entity, tail, room, _)| (entity, *room, tail)),
        );
        let origin = Vec2::ZERO;
        let direction = Vec2::X;
        let indexed = nearest_collision(origin, direction, 10.0, snake1, room, &index);
        let brute_force = nearest_collision_bruteforce(
            origin,
            direction,
            10.0,
            snake1,
            &room,
            query.iter(app.world()),
        );

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
        app.world_mut()
            .entity_mut(snake1)
            .insert(TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 1.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])));
        // snake2: horizontal across the snake1 movement sweep
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = TailPoints::new(VecDeque::from([
            (Vec2::new(50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(-50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]));
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
        app.world_mut()
            .entity_mut(snake1)
            .insert(TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 1.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])));
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = TailPoints::new(VecDeque::from([
            (Vec2::new(100.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(0.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]));
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
        let points2 = TailPoints::new(VecDeque::from([
            (Vec2::new(0.0, 50.0), Direction::Up),
            (Vec2::new(0.0, -50.0), Direction::Up),
        ]));
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
        let points2 = TailPoints::new(VecDeque::from([
            (Vec2::new(100.0, 10.0), Direction::Right),
            (Vec2::new(0.0, 10.0), Direction::Right),
        ]));
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
            TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 4.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut()
            .entity_mut(snake2)
            .insert(TailPoints::new(VecDeque::from([
                (Vec2::new(100.0, 2.0), Direction::Right),
                (Vec2::new(6.0, 2.0), Direction::Right),
            ])));

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
            TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 0.85), Direction::Up),
                (Vec2::ZERO, Direction::Up),
                (Vec2::new(-100.0, 0.0), Direction::Right),
            ])),
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
            TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 0.85), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut()
            .entity_mut(snake2)
            .insert(TailPoints::new(VecDeque::from([
                (Vec2::new(50.0, 0.4), Direction::Right),
                (Vec2::new(-50.0, 0.4), Direction::Right),
            ])));

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
        let points = TailPoints::new(VecDeque::from([
            (Vec2::new(-COLLISION_DISTANCE / 2.0, 50.0), Direction::Left),
            (Vec2::new(10.0, 50.0), Direction::Left),
            (Vec2::new(10.0, 100.0), Direction::Down),
            (Vec2::new(0.0, 100.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]));
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
        app.world_mut()
            .entity_mut(snake)
            .insert(TailPoints::new(VecDeque::from([
                (Vec2::new(60.0, 0.0), Direction::Right),
                (Vec2::new(0.0, 0.0), Direction::Right),
            ])));

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
            TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 4.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut()
            .entity_mut(snake2)
            .insert(TailPoints::new(VecDeque::from([
                (Vec2::new(50.0, 2.0), Direction::Right),
                (Vec2::new(-50.0, 2.0), Direction::Right),
            ])));

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
        app.world_mut()
            .entity_mut(snake1)
            .insert(TailPoints::new(VecDeque::from([
                (Vec2::new(0.0, 1.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = TailPoints::new(VecDeque::from([
            (Vec2::new(50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(-50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]));
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
}
