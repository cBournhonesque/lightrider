//! We compute collisions causing death only on the server.
use bevy::prelude::*;
use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{DeathReason, RoomId, SnakeCollision, Speed, TailPoints};
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

// NOTE: IMPORTANT
// because we do the ray cast with a small offset, we need to make sure that the collision distance is big enough
// that the snake cannot 'jump' over the obstacle in one movement update
// TODO: this might be problematic for fast moving snakes
//  - a solution might be to run the fixed update many times?
//  - or another solution is to start the raycast a bit further away? (but allow multiple hits)
// Collision of 1 pixel.
pub const COLLISION_DISTANCE: f32 = 1.0;

pub(crate) fn snake_collisions(
    tails: Query<(Entity, &TailPoints, &RoomId, &Speed)>,
    mut writer: MessageWriter<SnakeCollision>,
) {
    for (entity, tail, room, speed) in tails.iter() {
        let direction = tail.front().1.delta();
        let sweep_distance = speed.0.max(COLLISION_DISTANCE);
        let origin =
            tail.front().0 - direction * sweep_distance + direction * COLLISION_DISTANCE / 1000.0;
        trace!(head = ?tail.front().0, direction = ?tail.front().1, "Collision ray cast");
        if let Some(killer) =
            nearest_collision(origin, direction, sweep_distance, entity, room, &tails)
        {
            debug!(?entity, ?killer, "Collision");
            writer.write(SnakeCollision {
                killed: entity,
                killer,
                reason: if killer == entity {
                    DeathReason::Suicide
                } else {
                    DeathReason::Collision
                },
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

fn nearest_collision(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    main: Entity,
    room: &RoomId,
    tails: &Query<(Entity, &TailPoints, &RoomId, &Speed)>,
) -> Option<Entity> {
    let mut nearest: Option<(f32, Entity)> = None;
    for (other_entity, other_tail, other_room, _) in tails.iter() {
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

            if other_entity == main && distance <= COLLISION_DISTANCE / 1000.0 {
                continue;
            }
            if nearest.map_or(true, |(nearest_distance, _)| distance < nearest_distance) {
                nearest = Some((distance, other_entity));
            }
        }
    }
    nearest.map(|(_, entity)| entity)
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
    fn test_normal_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut()
            .entity_mut(snake1)
            .insert(TailPoints(VecDeque::from([
                (Vec2::new(0.0, 1.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])));
        // snake2: horizontal across the snake1 movement sweep
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
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
            .insert(TailPoints(VecDeque::from([
                (Vec2::new(0.0, 1.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])));
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
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
        let points2 = TailPoints(VecDeque::from([
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
        let points2 = TailPoints(VecDeque::from([
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
            TailPoints(VecDeque::from([
                (Vec2::new(0.0, 4.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut()
            .entity_mut(snake2)
            .insert(TailPoints(VecDeque::from([
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
    fn test_self_collision() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(ColliderPlugin);
        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        let points = TailPoints(VecDeque::from([
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
            .insert(TailPoints(VecDeque::from([
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
            TailPoints(VecDeque::from([
                (Vec2::new(0.0, 4.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut()
            .entity_mut(snake2)
            .insert(TailPoints(VecDeque::from([
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
            .insert(TailPoints(VecDeque::from([
                (Vec2::new(0.0, 1.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
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
