use bevy::prelude::{DetectChanges, Entity, Event, EventWriter, Query, Ref};
use bevy_xpbd_2d::parry::shape::SharedShape;
use bevy_xpbd_2d::prelude::{Collider, SpatialQuery, SpatialQueryFilter};
use tracing::{debug, info, trace};

use shared::network::protocol::prelude::TailPoints;

use crate::collision::layers::CollideLayer;

/// Update the collider for each snake that moves
pub(crate) fn update_collider(
    mut tails: Query<(Ref<TailPoints>, &mut Collider)>
) {
    for (tail, mut collider) in tails.iter_mut() {
        if tail.is_changed() && !tail.is_added() {
            let points = tail.points_front_to_back();
            trace!(?points, "Updating collider");
            *collider = Collider::from(SharedShape::polyline(points, None));
        }
    }
}

#[derive(Event, Debug, PartialEq)]
pub struct SnakeCollisionEvent {
    pub killed: Entity,
    pub killer: Entity,
}

#[derive(Event, Debug, PartialEq)]
pub struct SnakeFrictionEvent {
    pub main: Entity,
    pub other: Entity,
    pub distance: f32,
}

pub const COLLISION_DISTANCE: f32 = 0.1;
pub const MAX_FRICTION_DISTANCE: f32 = 5.0;

pub(crate) fn snake_collisions(
    spatial_query: SpatialQuery,
    tails: Query<(Entity, &TailPoints)>,
    mut writer: EventWriter<SnakeCollisionEvent>,
) {
    for (entity, tail) in tails.iter() {
        let filter = SpatialQueryFilter::new()
            .with_masks([CollideLayer::Player])
            .without_entities([entity]);
        trace!(head = ?tail.front().0, direction = ?tail.front().1, "Collision Ray cast");
        if let Some(collision) = spatial_query.cast_ray(
            tail.front().0,
            tail.front().1.delta(),
            COLLISION_DISTANCE,
            false,
            filter
        ) {
            // only send the event if the collision is perpendicular
            if collision.normal.dot(tail.front().1.delta()) != 0.0 {
                info!(?collision, "Collision!");
                writer.send(SnakeCollisionEvent {
                    killed: entity,
                    killer: collision.entity,
                });
            }
        }
    }
}

pub(crate) fn snake_friction(
    spatial_query: SpatialQuery,
    tails: Query<(Entity, &TailPoints)>,
    mut writer: EventWriter<SnakeFrictionEvent>,
) {
    for (entity, tail) in tails.iter() {
        let filter = SpatialQueryFilter::new()
            .with_masks([CollideLayer::Player])
            .without_entities([entity]);
        trace!(head = ?tail.front().0, direction = ?tail.front().1, "Friction Ray cast");
        let left_ray_cast = spatial_query.cast_ray(
            tail.front().0,
            tail.front().1.delta().perp(),
            MAX_FRICTION_DISTANCE,
            false,
            filter.clone()
        );
        let right_ray_cast = spatial_query.cast_ray(
            tail.front().0,
            -tail.front().1.delta().perp(),
            MAX_FRICTION_DISTANCE,
            false,
            filter
        );
        if left_ray_cast.is_some() || right_ray_cast.is_some() {
            let (distance, other) = left_ray_cast.map_or_else(
                || (right_ray_cast.unwrap().time_of_impact, right_ray_cast.unwrap().entity),
                |l| right_ray_cast.map_or_else(
                    || (l.time_of_impact, l.entity),
                    |r| if l.time_of_impact < r.time_of_impact {
                        (l.time_of_impact, l.entity)
                    } else {
                        (r.time_of_impact, r.entity)
                    }
                )
            );
            info!(?entity, ?distance, ?other, "Friction!");
            writer.send(SnakeFrictionEvent {
                main: entity,
                other,
                distance,
            });
        }
    }

}

#[cfg(test)]
mod tests {
    #![allow(unused_variables)]
    use std::collections::VecDeque;

    use bevy::prelude::*;
    use shared::network::protocol::prelude::Direction;

    use crate::network::bundle::snake::SnakeBundle;

    use super::*;

    #[test]
    fn test_normal_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::collision::CollisionPlugin);
        // snake1: [0, -100] -> [0, 0]
        let snake1 = app.world.spawn(SnakeBundle::default()).id();
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world.spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(-50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]));
        let collider2 = Collider::from(SharedShape::polyline(points2.points_front_to_back(), None));
        app.world.entity_mut(snake2).insert((points2, collider2));

        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<SnakeCollisionEvent>>().unwrap().drain().collect::<Vec<_>>(),
            vec![SnakeCollisionEvent {
                killed: snake1,
                killer: snake2,
            }])
        ;
    }

    #[test]
    fn test_edges_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::collision::CollisionPlugin);
        // snake1: [0, -100] -> [0, 0]
        let snake1 = app.world.spawn(SnakeBundle::default()).id();
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world.spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(100.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(0.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]));
        let collider2 = Collider::from(SharedShape::polyline(points2.points_front_to_back(), None));
        app.world.entity_mut(snake2).insert((points2, collider2));

        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<SnakeCollisionEvent>>().unwrap().drain().collect::<Vec<_>>(),
            vec![SnakeCollisionEvent {
                killed: snake1,
                killer: snake2,
            }]);
    }

    #[test]
    fn test_parallel_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::collision::CollisionPlugin);
        // snake1: [0, -100] -> [0, 0]
        let snake1 = app.world.spawn(SnakeBundle::default()).id();
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world.spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 50.0), Direction::Up),
            (Vec2::new(0.0, -50.0), Direction::Up),
        ]));
        let collider2 = Collider::from(SharedShape::polyline(points2.points_front_to_back(), None));
        app.world.entity_mut(snake2).insert((points2, collider2));

        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<SnakeCollisionEvent>>().unwrap().drain().collect::<Vec<_>>(),
            vec![]);
    }

    #[test]
    fn test_no_collision() {

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(crate::collision::CollisionPlugin);
        // snake1: [0, -100] -> [0, 0]
        let snake1 = app.world.spawn(SnakeBundle::default()).id();
        // snake2: [0, 0] -> [100, 0]
        let snake2 = app.world.spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(100.0, 10.0), Direction::Right),
            (Vec2::new(0.0, 10.0), Direction::Right),
        ]));
        let collider2 = Collider::from(SharedShape::polyline(points2.points_front_to_back(), None));
        app.world.entity_mut(snake2).insert((points2, collider2));
        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<SnakeCollisionEvent>>().unwrap().drain().collect::<Vec<_>>(),
            vec![]
        );
    }

}