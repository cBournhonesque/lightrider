use std::time::Duration;
use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;
use bevy_turborand::prelude::*;
use bevy_xpbd_2d::prelude::{SpatialQuery, SpatialQueryFilter};
use shared::collision::collider::ColliderSet;
use shared::collision::layers::CollideLayer;
use shared::map::{MapMarker, MapSize};
use shared::network::bundle::food::FoodBundle;
use shared::network::protocol::prelude::*;
use shared::network::protocol::Replicate;

pub struct FoodPlugin;

pub const FOOD_SPAWN_INTERVAL: Duration = Duration::from_secs(1);
// pub const MAX_FOOD_COUNT: usize = 100;
pub const TAIL_GROW_SIZE: f32 = 20.0;


// spawn food
fn spawn_food(
    mut commands: Commands,
    mut map: Query<(&MapSize, &mut RngComponent), With<MapMarker>>
) {
    let (map_size, mut rng) = map.single_mut();

    let x = rng.f32_normalized() * map_size.width * 0.5;
    let y = rng.f32_normalized() * map_size.height * 0.5;
    let pos = Position(Vec2::new(x, y));
    commands.spawn(
        (FoodBundle::new(pos), Replicate::default())
    );
}

// TODO: handle two players colliding with the same food at the same time
// TODO: after the first collision is detected, remove the collider on the food!
//  or set the food as 'dying'? maybe stop replicating it and then despawn?
/// System that handles a snake eating a food
fn food_collision(
    spatial_query: SpatialQuery,
    tails: Query<(Entity, &TailPoints)>,
    mut writer: EventWriter<FoodCollision>,
) {
    for (entity, tail) in tails.iter() {
        // the player can collide with itself!
        let filter = SpatialQueryFilter::from_mask(CollideLayer::Food);
        trace!(head = ?tail.front().0, direction = ?tail.front().1, "Food Collision Ray cast");
        if let Some(collision) = spatial_query.cast_ray(
            // IMPORTANT: add an epsilon otherwise the snake will collide with itself
            // (even if we have filter = food ?)
            tail.front().0 + tail.front().1.delta(),
            Direction2d::new_unchecked(tail.front().1.delta()),
            // we sent the distance to 0.0, because we just need to check if we're inside a food collider
            0.0,
            true,
            filter
        ) {
            info!(?collision, "Food Collision!");
            writer.send(FoodCollision {
                snake: entity,
                food: collision.entity,
            });
        }
    }
}

fn grow_tail(
    mut tails: Query<&mut TailLength>,
    mut events: EventReader<FoodCollision>,
) {
    for event in events.read() {
        if let Ok(mut tail_length) = tails.get_mut(event.snake){
            tail_length.target_size += TAIL_GROW_SIZE;
        }
    }
}

fn despawn_food(
    mut commands: Commands,
    mut events: EventReader<FoodCollision>,
) {
    for event in events.read() {
        if let Some(mut entity_command) = commands.get_entity(event.food) {
            // stop replicating the food
            entity_command.remove::<Replicate>();
            // despawn the food on the server side only (on the client side, we will run an animation)
            entity_command.despawn();
        }
    }
}

impl Plugin for FoodPlugin {
    fn build(&self, app: &mut App) {
        // EVENTS
        app.add_event::<FoodCollision>();
        // SYSTEMS
        // TODO: maybe run this before food collision?
        app.add_systems(Update, spawn_food.run_if(on_timer(FOOD_SPAWN_INTERVAL)),);

        app.add_systems(Update, (
            food_collision.in_set(ColliderSet::ComputeCollision),
            (grow_tail, despawn_food).after(food_collision),
        ));
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_variables)]
    use std::collections::VecDeque;
    use bevy::prelude::*;
    use shared::network::bundle::snake::SnakeBundle;
    use shared::network::protocol::prelude::Direction;

    use super::*;

    #[test]
    fn test_collision_not_at_zero() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);
        // snake: vertical, pointing up
        let snake = app.world.spawn(SnakeBundle::default()).id();
        let points = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 200.0), Direction::Up),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]));
        app.world.entity_mut(snake).insert(points);
        // food: in front of snake
        let food = app.world.spawn(FoodBundle::new(Position(Vec2::new(0.0, 201.0)))).id();

        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<FoodCollision>>().unwrap().drain().collect::<Vec<_>>(),
            vec![FoodCollision {
                snake,
                food,
            }])
        ;
    }

    #[test]
    fn test_collision_at_zero() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);
        // snake: vertical, pointing up
        let snake = app.world.spawn(SnakeBundle::default()).id();
        // food: in front of snake
        let food = app.world.spawn(FoodBundle::new(Position(Vec2::new(0.0, 1.0)))).id();

        app.update();

        assert_eq!(
            app.world.get_resource_mut::<Events<FoodCollision>>().unwrap().drain().collect::<Vec<_>>(),
            vec![FoodCollision {
                snake,
                food,
            }])
        ;
    }
}