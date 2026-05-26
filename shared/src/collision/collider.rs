use bevy::prelude::*;

use crate::config::GameConfig;
use crate::movement::SimulationSet;
use crate::network::protocol::prelude::{RoomId, TailPoints};
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
    // we will only compute the friction of predicted/interpolated snakes
    config: Res<GameConfig>,
    tails: Query<(Entity, &TailPoints, &RoomId), Simulated>,
    mut writer: MessageWriter<SnakeFrictionEvent>,
) {
    let max_distance = config.movement.boost_distance;
    if max_distance <= 0.0 {
        return;
    }
    for (entity, tail, room) in tails.iter() {
        let origin = tail.front().0;
        let direction = tail.front().1.delta();
        let left_hit =
            nearest_tail_ray_hit(origin, direction.perp(), max_distance, entity, room, &tails);
        let right_hit = nearest_tail_ray_hit(
            origin,
            -direction.perp(),
            max_distance,
            entity,
            room,
            &tails,
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
    room: &RoomId,
    tails: &Query<(Entity, &TailPoints, &RoomId), Simulated>,
) -> Option<(f32, Entity)> {
    let mut nearest: Option<(f32, Entity)> = None;
    for (other_entity, other_tail, other_room) in tails.iter() {
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

    use crate::network::bundle::snake::SnakeBundle;
    use crate::network::protocol::prelude::Direction;

    use super::*;

    fn run_fixed_update(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
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
}
