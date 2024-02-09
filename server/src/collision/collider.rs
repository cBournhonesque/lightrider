use bevy::prelude::*;
use bevy_xpbd_2d::parry::shape::SharedShape;
use bevy_xpbd_2d::PhysicsStepSet;
use bevy_xpbd_2d::prelude::*;
use tracing::{debug, info, trace};

use shared::network::protocol::prelude::{SnakeCollision, TailPoints};

use crate::collision::layers::CollideLayer;

pub struct ColliderPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy, Reflect)]
pub enum ColliderSet {
    // update colliders
    UpdateColliders,
    // compute all collision information
    ComputeCollision,
}

impl Plugin for ColliderPlugin {
    fn build(&self, app: &mut App) {
        // plugins
        app.add_plugins(PhysicsSetupPlugin::new(Update));
        app.add_plugins(SpatialQueryPlugin::new(Update));
        // events
        app.add_event::<SnakeFrictionEvent>();
        // sets
        app.configure_sets(Update, (ColliderSet::UpdateColliders, PhysicsStepSet::SpatialQuery, ColliderSet::ComputeCollision).chain());
        // systems
        app.add_systems(Update, (
            update_collider.in_set(ColliderSet::UpdateColliders),
            (snake_collisions, snake_friction)
                .in_set(ColliderSet::ComputeCollision),
        ));

        // reflect
        app.register_type::<ColliderSet>();
    }
}

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
pub struct SnakeFrictionEvent {
    pub main: Entity,
    pub other: Entity,
    pub distance: f32,
}

// NOTE: IMPORTANT
// because we do the ray cast with a small offset, we need to make sure that the collision distance is big enough
// that the snake cannot 'jump' over the obstable in one movement update
// TODO: this might be problematic for fast moving snakes
//  - a solution might be to run the fixed update many times?
//  - or another solution is to start the raycast a bit further away? (but allow multiple hits)
// Collision of 1 pixel.
pub const COLLISION_DISTANCE: f32 = 1.0;
pub const MAX_FRICTION_DISTANCE: f32 = 5.0;

pub(crate) fn snake_collisions(
    spatial_query: SpatialQuery,
    tails: Query<(Entity, &TailPoints)>,
    mut writer: EventWriter<SnakeCollision>,
) {
    for (entity, tail) in tails.iter() {
        // the player can collide with itself!
        let filter = SpatialQueryFilter::new()
            .with_masks([CollideLayer::Player]);
        trace!(head = ?tail.front().0, direction = ?tail.front().1, "Collision Ray cast");
        if let Some(collision) = spatial_query.cast_ray(
            // NOTE: important!
            // offset the head by epsilon to avoid a self-collision on the head
            tail.front().0 + tail.front().1.delta() * COLLISION_DISTANCE / 1000.0,
            tail.front().1.delta(),
            COLLISION_DISTANCE,
            false,
            filter
        ) {
            trace!(normal = ?collision.normal.dot(tail.front().1.delta()), "Possible collision: {:?}", collision);
            // only send the event if the collision is perpendicular
            if collision.normal.dot(tail.front().1.delta()) != 0.0 {
                debug!(?collision, "Collision!");
                writer.send(SnakeCollision {
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
        app.add_plugins(ColliderPlugin);
        // snake1: vertical, pointing up
        let snake1 = app.world.spawn(SnakeBundle::default()).id();
        // snake2: horizontal in front of the snake1
        let snake2 = app.world.spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
            (Vec2::new(-50.0, COLLISION_DISTANCE / 2.0), Direction::Right),
        ]));
        let collider2 = Collider::from(SharedShape::polyline(points2.points_front_to_back(), None));
        app.world.entity_mut(snake2).insert((points2, collider2));

        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<SnakeCollision>>().unwrap().drain().collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake1,
                killer: snake2,
            }])
        ;
    }

    #[test]
    fn test_edges_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);
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
            app.world.get_resource_mut::<Events<SnakeCollision>>().unwrap().drain().collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake1,
                killer: snake2,
            }]);
    }

    #[test]
    fn test_parallel_collision() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);
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
            app.world.get_resource_mut::<Events<SnakeCollision>>().unwrap().drain().collect::<Vec<_>>(),
            vec![]);
    }

    #[test]
    fn test_no_collision() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);
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
            app.world.get_resource_mut::<Events<SnakeCollision>>().unwrap().drain().collect::<Vec<_>>(),
            vec![]
        );
    }

    #[test]
    fn test_self_collision() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);
        let snake = app.world.spawn(SnakeBundle::default()).id();
        let points = TailPoints(VecDeque::from([
            (Vec2::new(COLLISION_DISTANCE / 2.0, 50.0), Direction::Left),
            (Vec2::new(10.0, 50.0), Direction::Left),
            (Vec2::new(10.0, 100.0), Direction::Down),
            (Vec2::new(0.0, 100.0), Direction::Right),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]));
        let collider = Collider::from(SharedShape::polyline(points.points_front_to_back(), None));
        app.world.entity_mut(snake).insert((points, collider));
        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<SnakeCollision>>().unwrap().drain().collect::<Vec<_>>(),
            vec![SnakeCollision {
                killed: snake,
                killer: snake,
            }]
        );
    }

}