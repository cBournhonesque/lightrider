use crate::rooms::{
    add_replicated_entity_to_room, remove_replicated_entity_from_room, RoomDirectory,
};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use bevy_turborand::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{NetworkTarget, Replicate, ReplicationSender};
use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::map::{MapMarker, MapSize};
use shared::network::bundle::food::FoodBundle;
use shared::network::protocol::prelude::*;

pub struct FoodPlugin;

// spawn food
fn spawn_food(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: Local<Option<Timer>>,
    mut maps: Query<(&RoomId, &MapSize, &mut RngComponent), With<MapMarker>>,
    food: Query<&RoomId, With<FoodMarker>>,
    config: Res<GameConfig>,
    rooms: Res<RoomDirectory>,
    pending_clients: Query<(), (With<ClientOf>, Without<ReplicationSender>)>,
) {
    if !pending_clients.is_empty() {
        return;
    }

    let timer =
        timer.get_or_insert_with(|| Timer::new(config.food.spawn_interval(), TimerMode::Repeating));
    if config.is_changed() {
        timer.set_duration(config.food.spawn_interval());
    }
    if !timer.tick(time.delta()).just_finished() {
        return;
    }
    for (room, map_size, mut rng) in &mut maps {
        let room_food_count = food.iter().filter(|food_room| *food_room == room).count();
        if room_food_count >= config.food.target_count {
            continue;
        }

        let x = rng.f32_normalized() * map_size.width * 0.5;
        let y = rng.f32_normalized() * map_size.height * 0.5;
        spawn_food_entity(&mut commands, &rooms, *room, Position(Vec2::new(x, y)));
    }
}

pub(crate) fn spawn_food_entity(
    commands: &mut Commands,
    rooms: &RoomDirectory,
    room: RoomId,
    position: Position,
) -> Entity {
    let food = commands
        .spawn((
            FoodBundle::new_in_room(position, room),
            Replicate::to_clients(NetworkTarget::All),
        ))
        .id();
    if let Some(lightyear_room) = rooms.lightyear_room(room) {
        add_replicated_entity_to_room(commands, lightyear_room, food);
    }
    food
}

fn attract_food_to_heads(
    config: Res<GameConfig>,
    tails: Query<(&TailPoints, &RoomId)>,
    mut food: Query<(&mut Position, &RoomId), With<FoodMarker>>,
) {
    let magnet_radius = config.food.magnet_radius.max(0.0);
    let magnet_speed = config.food.magnet_speed.max(0.0);
    if magnet_radius <= 0.0 || magnet_speed <= 0.0 {
        return;
    }

    for (mut position, food_room) in &mut food {
        let nearest_head = tails
            .iter()
            .filter(|(_, snake_room)| *snake_room == food_room)
            .map(|(tail, _)| tail.front().0)
            .filter(|head| head.distance(position.0) <= magnet_radius)
            .min_by(|left, right| {
                left.distance_squared(position.0)
                    .total_cmp(&right.distance_squared(position.0))
            });
        if let Some(head) = nearest_head {
            position.0 = magnetized_food_position(position.0, head, magnet_radius, magnet_speed);
        }
    }
}

pub fn magnetized_food_position(
    position: Vec2,
    head: Vec2,
    magnet_radius: f32,
    magnet_speed: f32,
) -> Vec2 {
    let to_head = head - position;
    let distance = to_head.length();
    if distance <= f32::EPSILON || distance > magnet_radius || magnet_speed <= 0.0 {
        return position;
    }
    position + to_head / distance * distance.min(magnet_speed)
}

// TODO: handle two players colliding with the same food at the same time
// TODO: after the first collision is detected, remove the collider on the food!
//  or set the food as 'dying'? maybe stop replicating it and then despawn?
/// System that handles a snake eating a food
fn food_collision(
    config: Res<GameConfig>,
    tails: Query<(Entity, &TailPoints, &RoomId)>,
    food: Query<(Entity, &Position, &RoomId), With<FoodMarker>>,
    mut writer: MessageWriter<FoodCollision>,
) {
    let mut eaten_food = EntityHashSet::default();
    for (snake, tail, room) in tails.iter() {
        let collision_point = tail.front().0;
        trace!(head = ?tail.front().0, direction = ?tail.front().1, "Food collision check");
        for (food_entity, position, food_room) in food.iter() {
            if food_room != room || eaten_food.contains(&food_entity) {
                continue;
            }
            if collision_point.distance(position.0) <= config.food.radius {
                info!(?snake, ?food_entity, "Food collision");
                eaten_food.insert(food_entity);
                writer.write(FoodCollision {
                    snake,
                    food: food_entity,
                });
                break;
            }
        }
    }
}

fn grow_tail(
    config: Res<GameConfig>,
    mut tails: Query<(&mut TailLength, &mut FoodBoost)>,
    snake_players: Query<&HasPlayer>,
    mut scores: Query<&mut PlayerScore>,
    mut stats: Query<&mut PlayerStats>,
    mut events: MessageReader<FoodCollision>,
) {
    for event in events.read() {
        if let Ok((mut tail_length, mut food_boost)) = tails.get_mut(event.snake) {
            tail_length.target_size += config.food.tail_growth;
            food_boost.0 += config.movement.food_boost_acceleration.max(0.0);
            if let Ok(has_player) = snake_players.get(event.snake) {
                if let Ok(mut score) = scores.get_mut(has_player.0) {
                    *score = PlayerScore::from_tail_length(
                        tail_length.target_size,
                        config.movement.starting_tail_length,
                    );
                }
                if let Ok(mut stats) = stats.get_mut(has_player.0) {
                    stats.food_eaten = stats.food_eaten.saturating_add(1);
                }
            }
        }
    }
}

fn despawn_food(
    mut commands: Commands,
    rooms: Res<RoomDirectory>,
    food_rooms: Query<&RoomId, With<FoodMarker>>,
    mut events: MessageReader<FoodCollision>,
) {
    for event in events.read() {
        if let Ok(room) = food_rooms.get(event.food) {
            if let Some(lightyear_room) = rooms.lightyear_room(*room) {
                remove_replicated_entity_from_room(&mut commands, lightyear_room, event.food);
            }
        }
        if let Ok(mut entity_command) = commands.get_entity(event.food) {
            // TODO: provide a way to stop replicating the entity via a command!
            //  (so that we don't have to wait for the handle_replicate_remove system to run)
            //  probably via a command?
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
        app.add_message::<FoodCollision>();
        // SYSTEMS
        // TODO: maybe run this before food collision?
        app.init_resource::<GameConfig>();
        app.init_resource::<RoomDirectory>();
        app.add_systems(Update, spawn_food);

        app.add_systems(
            FixedUpdate,
            (
                (attract_food_to_heads, food_collision)
                    .chain()
                    .in_set(ColliderSet::ComputeCollision),
                (grow_tail, despawn_food).after(food_collision),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_variables)]
    use bevy::prelude::*;
    use lightyear::prelude::{Interpolated, Replicated};
    use shared::movement::MovementPlugin;
    use shared::network::bundle::snake::SnakeBundle;
    use shared::network::protocol::prelude::Direction;
    use shared::utils::SimulationAuthority;
    use std::collections::VecDeque;

    use super::*;

    fn run_fixed_update(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
    }

    #[test]
    fn test_collision_not_at_zero() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);
        // snake: vertical, pointing up
        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        let points = TailPoints(VecDeque::from([
            (Vec2::new(0.0, 200.0), Direction::Up),
            (Vec2::new(0.0, 0.0), Direction::Up),
        ]));
        app.world_mut().entity_mut(snake).insert(points);
        // food: in front of snake
        let food = app
            .world_mut()
            .spawn(FoodBundle::new(Position(Vec2::new(0.0, 201.0))))
            .id();

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<FoodCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![FoodCollision { snake, food }]
        );
    }

    #[test]
    fn test_collision_at_zero() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);
        // snake: vertical, pointing up
        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        // food: in front of snake
        let food = app
            .world_mut()
            .spawn(FoodBundle::new(Position(Vec2::new(0.0, 1.0))))
            .id();

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<FoodCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![FoodCollision { snake, food }]
        );
    }

    #[test]
    fn test_collision_ignores_food_in_other_rooms() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);

        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        let food = app
            .world_mut()
            .spawn(FoodBundle::new_in_room(
                Position(Vec2::new(0.0, 1.0)),
                RoomId(1),
            ))
            .id();

        run_fixed_update(&mut app);

        assert_eq!(
            app.world_mut()
                .get_resource_mut::<Messages<FoodCollision>>()
                .unwrap()
                .drain()
                .collect::<Vec<_>>(),
            vec![]
        );
        assert_ne!(
            app.world().entity(snake).get::<RoomId>(),
            app.world().entity(food).get::<RoomId>()
        );
    }

    #[test]
    fn test_food_pickup_updates_player_score_from_tail_length() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);

        let player = app
            .world_mut()
            .spawn((PlayerScore::default(), PlayerStatus::Alive))
            .id();
        let snake = app
            .world_mut()
            .spawn((SnakeBundle::default(), HasPlayer(player)))
            .id();
        let food = app
            .world_mut()
            .spawn(FoodBundle::new(Position(Vec2::new(0.0, 1.0))))
            .id();

        run_fixed_update(&mut app);

        assert_eq!(
            app.world().entity(player).get::<PlayerScore>(),
            Some(&PlayerScore::from_tail_length(
                shared::config::MovementConfig::default().starting_tail_length
                    + GameConfig::default().food.tail_growth,
                shared::config::MovementConfig::default().starting_tail_length,
            ))
        );
        assert!(app.world().get_entity(food).is_err());
        assert_eq!(
            app.world()
                .entity(snake)
                .get::<TailLength>()
                .unwrap()
                .target_size,
            shared::config::MovementConfig::default().starting_tail_length
                + GameConfig::default().food.tail_growth
        );
        assert_eq!(
            app.world().entity(snake).get::<FoodBoost>(),
            Some(&FoodBoost(
                GameConfig::default().movement.food_boost_acceleration
            ))
        );
    }

    #[test]
    fn magnetized_food_moves_toward_head_without_overshooting() {
        assert_eq!(
            magnetized_food_position(Vec2::ZERO, Vec2::new(20.0, 0.0), 30.0, 5.0),
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(
            magnetized_food_position(Vec2::ZERO, Vec2::new(3.0, 0.0), 30.0, 5.0),
            Vec2::new(3.0, 0.0)
        );
        assert_eq!(
            magnetized_food_position(Vec2::ZERO, Vec2::new(40.0, 0.0), 30.0, 5.0),
            Vec2::ZERO
        );
    }

    #[test]
    fn replicated_authoritative_snake_can_pick_up_food_after_moving() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameConfig>();
        app.add_plugins(MovementPlugin);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);

        let snake = app
            .world_mut()
            .spawn((
                SnakeBundle::default(),
                Replicated,
                Interpolated,
                SimulationAuthority,
            ))
            .id();
        let food = app
            .world_mut()
            .spawn(FoodBundle::new(Position(Vec2::new(0.0, 1.0))))
            .id();

        run_fixed_update(&mut app);

        assert!(app.world().get_entity(food).is_err());
        assert_eq!(
            app.world()
                .entity(snake)
                .get::<TailLength>()
                .unwrap()
                .target_size,
            shared::config::MovementConfig::default().starting_tail_length
                + GameConfig::default().food.tail_growth
        );
    }
}
