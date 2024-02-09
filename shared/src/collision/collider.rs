use bevy::prelude::*;
use bevy_xpbd_2d::parry::shape::SharedShape;
use bevy_xpbd_2d::PhysicsStepSet;
use bevy_xpbd_2d::prelude::*;
use lightyear::prelude::client::Confirmed;
use tracing::{trace};

use crate::network::protocol::prelude::{TailPoints};

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
            snake_friction.in_set(ColliderSet::ComputeCollision),
        ));

        // reflect
        app.register_type::<ColliderSet>();
    }
}

/// Update the collider for each snake that moves
pub(crate) fn update_collider(
    mut tails: Query<(Ref<TailPoints>, &mut Collider), Without<Confirmed>>
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

pub const MAX_FRICTION_DISTANCE: f32 = 20.0;

/// Friction is computed both on the client and the server because it influences movement
pub(crate) fn snake_friction(
    spatial_query: SpatialQuery,
    // we will only compute the friction of predicted/interpolated snakes
    tails: Query<(Entity, &TailPoints), Without<Confirmed>>,
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
            trace!(?entity, ?distance, ?other, "Friction!");
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
    use crate::network::protocol::prelude::Direction;

    use crate::network::bundle::snake::SnakeBundle;

    use super::*;

    #[test]
    fn test_normal_friction() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(ColliderPlugin);
        // snake1: vertical, pointing up
        let snake1 = app.world.spawn(SnakeBundle::default()).id();
        // snake2: vertical on the left of snake1
        let snake2 = app.world.spawn(SnakeBundle::default()).id();
        let points2 = TailPoints(VecDeque::from([
            (Vec2::new(-MAX_FRICTION_DISTANCE / 1.5, 0.0), Direction::Up),
            (Vec2::new(-MAX_FRICTION_DISTANCE / 1.5, -100.0), Direction::Up),
        ]));
        let collider2 = Collider::from(SharedShape::polyline(points2.points_front_to_back(), None));
        app.world.entity_mut(snake2).insert((points2, collider2));
        // snake3: vertical on the right of snake1, closer than snake 2
        let snake3 = app.world.spawn(SnakeBundle::default()).id();
        let points3 = TailPoints(VecDeque::from([
            (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, 0.0), Direction::Up),
            (Vec2::new(MAX_FRICTION_DISTANCE / 2.0, -100.0), Direction::Up),
        ]));
        let collider3 = Collider::from(SharedShape::polyline(points3.points_front_to_back(), None));
        app.world.entity_mut(snake3).insert((points3, collider3));

        app.update();

        let mut result = app.world.get_resource_mut::<Events<SnakeFrictionEvent>>().unwrap().drain().collect::<Vec<_>>();
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
            }
        ];
        expected.sort_by(|a, b| a.main.partial_cmp(&b.main).unwrap());

        assert_eq!(result, expected);
    }

}