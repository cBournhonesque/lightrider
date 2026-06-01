use bevy::ecs::query::Or;
use bevy::prelude::*;
use lightyear::prelude::Interpolated;

use crate::config::GameConfig;
use crate::movement::SimulationSet;
use crate::network::protocol::prelude::{RoomId, TailPoints};
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

pub const MAX_FRICTION_DISTANCE: f32 = 20.0;

/// Friction is computed both on the client and the server because it influences movement.
pub(crate) fn snake_friction(
    // Only predicted/client-local or server-authoritative snakes receive boost events, but
    // interpolated remote tails are still valid obstacles for client-side prediction.
    config: Res<GameConfig>,
    boosted: Query<(Entity, &TailPoints, &RoomId), Simulated>,
    tails: Query<(Entity, &TailPoints, &RoomId), Or<(Simulated, With<Interpolated>)>>,
    mut writer: MessageWriter<SnakeFrictionEvent>,
) {
    let max_distance = config.movement.boost_distance;
    if max_distance <= 0.0 {
        return;
    }
    let tail_index = TailSpatialIndex::from_tails(
        tails
            .iter()
            .map(|(entity, tail, room)| (entity, *room, tail)),
    );
    for (entity, tail, room) in boosted.iter() {
        let origin = tail.front().0;
        let direction = tail.front().1.delta();
        let left_hit = nearest_tail_ray_hit(
            origin,
            direction.perp(),
            max_distance,
            entity,
            *room,
            &tail_index,
        );
        let right_hit = nearest_tail_ray_hit(
            origin,
            -direction.perp(),
            max_distance,
            entity,
            *room,
            &tail_index,
        );

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

#[cfg(test)]
fn nearest_tail_ray_hit_bruteforce<'a>(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    excluded: Entity,
    room: &RoomId,
    tails: impl IntoIterator<Item = (Entity, &'a TailPoints, &'a RoomId)>,
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
    use lightyear::prelude::{Interpolated, Predicted, Replicated};

    use crate::network::bundle::snake::SnakeBundle;
    use crate::network::protocol::prelude::Direction;

    use super::*;

    fn run_fixed_update(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
    }

    #[test]
    fn indexed_friction_query_matches_bruteforce() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let room = RoomId(1);
        let snake1 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake1).insert((
            room,
            TailPoints(VecDeque::from([
                (Vec2::new(0.0, 0.0), Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])),
        ));
        let snake2 = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake2).insert((
            room,
            TailPoints(VecDeque::from([
                (Vec2::new(8.0, 50.0), Direction::Up),
                (Vec2::new(8.0, -50.0), Direction::Up),
            ])),
        ));
        let other_room_snake = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(other_room_snake).insert((
            RoomId(2),
            TailPoints(VecDeque::from([
                (Vec2::new(2.0, 50.0), Direction::Up),
                (Vec2::new(2.0, -50.0), Direction::Up),
            ])),
        ));

        let mut query = app.world_mut().query::<(Entity, &TailPoints, &RoomId)>();
        let index = TailSpatialIndex::from_tails(
            query
                .iter(app.world())
                .map(|(entity, tail, room)| (entity, *room, tail)),
        );

        let origin = Vec2::ZERO;
        let direction = Vec2::X;
        let indexed = nearest_tail_ray_hit(origin, direction, 20.0, snake1, room, &index);
        let brute_force = nearest_tail_ray_hit_bruteforce(
            origin,
            direction,
            20.0,
            snake1,
            &room,
            query.iter(app.world()),
        );

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
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(-MAX_FRICTION_DISTANCE / 1.5, 0.0), Direction::Up),
            (
                Vec2::new(-MAX_FRICTION_DISTANCE / 1.5, -100.0),
                Direction::Up,
            ),
        ]));
        app.world_mut().entity_mut(snake2).insert(points2);
        // snake3: vertical on the right of snake1, closer than snake 2
        let snake3 = app.world_mut().spawn(SnakeBundle::default()).id();
        let points3 = TailPoints(VecDeque::from([
            (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, 0.0), Direction::Up),
            (
                Vec2::new(MAX_FRICTION_DISTANCE / 2.0, -100.0),
                Direction::Up,
            ),
        ]));
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
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, 0.0), Direction::Up),
            (
                Vec2::new(MAX_FRICTION_DISTANCE / 2.0, -100.0),
                Direction::Up,
            ),
        ]));
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
        app.world_mut()
            .entity_mut(predicted)
            .insert(TailPoints(VecDeque::from([
                (Vec2::ZERO, Direction::Up),
                (Vec2::new(0.0, -100.0), Direction::Up),
            ])));
        let remote = app
            .world_mut()
            .spawn((SnakeBundle::default(), Replicated, Interpolated))
            .id();
        app.world_mut()
            .entity_mut(remote)
            .insert(TailPoints(VecDeque::from([
                (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, 100.0), Direction::Up),
                (
                    Vec2::new(MAX_FRICTION_DISTANCE / 2.0, -100.0),
                    Direction::Up,
                ),
            ])));

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
}
