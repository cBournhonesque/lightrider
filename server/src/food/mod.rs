use crate::rooms::{add_replicated_entity_to_room, ClientRoom, RoomDirectory};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use bevy_rand::prelude::WyRand;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    InterpolationTarget, NetworkTarget, RemoteId, Replicate, ReplicationSender, Server,
    ServerMultiMessageSender,
};
use rand_core::Rng;
use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::map::{MapMarker, MapSize};
use shared::network::bundle::food::FoodBundle;
use shared::network::protocol::prelude::*;
use shared::spatial::{FoodPoint, FoodSpatialIndex};
use std::collections::HashSet;
use tracing::error;

pub struct FoodPlugin;

// spawn food
fn spawn_food(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: Local<Option<Timer>>,
    mut seeded_rooms: Local<HashSet<RoomId>>,
    mut maps: Query<(&RoomId, &MapSize, &mut WyRand), With<MapMarker>>,
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
    let spawn_regular_food = timer.tick(time.delta()).just_finished();
    for (room, map_size, mut rng) in &mut maps {
        let room_food_count = food.iter().filter(|food_room| *food_room == room).count();
        let target_count = config.food.spawn_target_count();
        if seeded_rooms.insert(*room) {
            for _ in 0..target_count.saturating_sub(room_food_count) {
                spawn_random_food(&mut commands, &rooms, *room, map_size, rng.as_mut());
            }
            continue;
        }
        if !spawn_regular_food || room_food_count >= target_count {
            continue;
        }

        spawn_random_food(&mut commands, &rooms, *room, map_size, rng.as_mut());
    }
}

fn spawn_random_food(
    commands: &mut Commands,
    rooms: &RoomDirectory,
    room: RoomId,
    map_size: &MapSize,
    rng: &mut WyRand,
) -> Entity {
    let x = f32_normalized(rng) * map_size.width * 0.5;
    let y = f32_normalized(rng) * map_size.height * 0.5;
    spawn_food_entity(commands, rooms, room, Position(Vec2::new(x, y)))
}

fn f32_normalized(rng: &mut WyRand) -> f32 {
    (rng.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
}

pub(crate) fn spawn_food_entity(
    commands: &mut Commands,
    rooms: &RoomDirectory,
    room: RoomId,
    position: Position,
) -> Entity {
    spawn_food_entity_inner(commands, rooms, room, position, None)
}

pub(crate) fn spawn_colored_food_entity(
    commands: &mut Commands,
    rooms: &RoomDirectory,
    room: RoomId,
    position: Position,
    color: FoodColor,
) -> Entity {
    spawn_food_entity_inner(commands, rooms, room, position, Some(color))
}

fn spawn_food_entity_inner(
    commands: &mut Commands,
    rooms: &RoomDirectory,
    room: RoomId,
    position: Position,
    color: Option<FoodColor>,
) -> Entity {
    let mut food_entity = commands.spawn((
        FoodBundle::new_in_room(position, room),
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::All),
    ));
    if let Some(color) = color {
        food_entity.insert(color);
    }
    let food = food_entity.id();
    if let Some(lightyear_room) = rooms.lightyear_room(room) {
        add_replicated_entity_to_room(commands, lightyear_room, food);
    }
    food
}

// TODO: handle two players colliding with the same food at the same time
// TODO: after the first collision is detected, remove the collider on the food!
//  or set the food as 'dying'? maybe stop replicating it and then despawn?
/// System that handles a snake eating a food
fn food_collision(
    config: Res<GameConfig>,
    tails: Query<(Entity, &SnakeHead, &RoomId)>,
    food: Query<(Entity, &Position, &RoomId), With<FoodMarker>>,
    mut writer: MessageWriter<FoodCollision>,
) {
    let mut eaten_food = EntityHashSet::default();
    let food_index =
        FoodSpatialIndex::from_food(food.iter().map(|(entity, position, room)| FoodPoint {
            entity,
            room: *room,
            position: position.0,
        }));
    for (snake, head, room) in tails.iter() {
        let collision_point = head.position;
        for food in food_index.within_radius(*room, collision_point, config.food.radius) {
            if eaten_food.contains(&food.entity) {
                continue;
            }
            eaten_food.insert(food.entity);
            writer.write(FoodCollision {
                snake,
                food: food.entity,
                food_position: food.position,
                head_position: collision_point,
            });
            break;
        }
    }
}

#[cfg(test)]
fn nearest_food_bruteforce(
    room: RoomId,
    head: Vec2,
    radius: f32,
    eaten_food: &EntityHashSet,
    food: impl IntoIterator<Item = FoodPoint>,
) -> Option<FoodPoint> {
    food.into_iter()
        .filter(|food| food.room == room)
        .filter(|food| !eaten_food.contains(&food.entity))
        .filter(|food| head.distance(food.position) <= radius)
        .min_by(|left, right| {
            left.position
                .distance_squared(head)
                .total_cmp(&right.position.distance_squared(head))
                .then_with(|| left.entity.to_bits().cmp(&right.entity.to_bits()))
        })
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

fn send_food_collision_messages(
    servers: Query<&Server>,
    sender: Option<ServerMultiMessageSender>,
    clients: Query<(&RemoteId, &ClientRoom), With<ClientOf>>,
    tails: Query<&RoomId, With<TailPoints>>,
    mut events: MessageReader<FoodCollision>,
) {
    let Some(mut sender) = sender else {
        for _ in events.read() {}
        return;
    };
    let Some(server) = servers.iter().next() else {
        for _ in events.read() {}
        return;
    };
    for event in events.read() {
        let Ok(room) = tails.get(event.snake) else {
            continue;
        };
        for (remote_id, client_room) in &clients {
            if client_room.room != *room {
                continue;
            }
            if let Err(error) =
                sender.send::<_, GameChannel>(event, server, &NetworkTarget::Single(remote_id.0))
            {
                error!(
                    ?error,
                    peer_id = ?remote_id.0,
                    ?event,
                    "failed to send confirmed food collision"
                );
            }
        }
    }
}

fn despawn_food(mut commands: Commands, mut events: MessageReader<FoodCollision>) {
    for event in events.read() {
        commands.entity(event.food).try_despawn();
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
                food_collision.in_set(ColliderSet::ComputeCollision),
                (grow_tail, send_food_collision_messages, despawn_food)
                    .chain()
                    .after(food_collision),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_variables)]
    use bevy::prelude::*;
    use lightyear::prelude::{Interpolated, Replicated};
    use rand_core::SeedableRng;
    use shared::movement::MovementPlugin;
    use shared::network::bundle::snake::SnakeBundle;
    use shared::utils::SimulationAuthority;

    use super::*;

    fn run_fixed_update(app: &mut App) {
        app.world_mut().run_schedule(FixedUpdate);
    }

    #[derive(Resource)]
    struct SpawnedFood(Entity);

    fn spawn_test_food(mut commands: Commands, rooms: Res<RoomDirectory>) {
        let food = spawn_food_entity(
            &mut commands,
            &rooms,
            RoomId(1),
            Position(Vec2::new(1.0, 2.0)),
        );
        commands.insert_resource(SpawnedFood(food));
    }

    #[test]
    fn spawned_food_uses_delayed_interpolation_despawn_path() {
        let mut app = App::new();
        app.init_resource::<RoomDirectory>();
        app.add_systems(Update, spawn_test_food);

        app.update();

        let food = app.world().resource::<SpawnedFood>().0;
        let food = app.world().entity(food);
        assert!(food.contains::<Replicate>());
        assert!(food.contains::<InterpolationTarget>());
    }

    #[test]
    fn spawn_food_seeds_new_rooms_to_target_count() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let mut config = GameConfig::default();
        config.food.target_count = 3;
        config.food.max_count = 3;
        app.insert_resource(config.clone());
        app.init_resource::<RoomDirectory>();
        app.add_systems(Update, spawn_food);
        app.world_mut().spawn((
            MapSize {
                width: config.arena.width,
                height: config.arena.height,
            },
            RoomId(7),
            MapMarker,
            WyRand::from_seed(7_u64.to_ne_bytes()),
        ));

        app.update();

        let mut food = app
            .world_mut()
            .query_filtered::<&RoomId, With<FoodMarker>>();
        assert_eq!(
            food.iter(app.world())
                .filter(|room| **room == RoomId(7))
                .count(),
            3
        );
    }

    #[test]
    fn indexed_food_query_matches_bruteforce() {
        let room = RoomId(1);
        let head = Vec2::ZERO;
        let near = Entity::from_bits(3);
        let farther = Entity::from_bits(4);
        let other_room = Entity::from_bits(5);
        let food = vec![
            FoodPoint {
                entity: farther,
                room,
                position: Vec2::new(4.0, 0.0),
            },
            FoodPoint {
                entity: near,
                room,
                position: Vec2::new(1.0, 0.0),
            },
            FoodPoint {
                entity: other_room,
                room: RoomId(2),
                position: Vec2::ZERO,
            },
        ];
        let index = FoodSpatialIndex::from_food(food.iter().copied());
        let eaten_food = EntityHashSet::default();

        assert_eq!(
            index.within_radius(room, head, 5.0).first().copied(),
            nearest_food_bruteforce(room, head, 5.0, &eaten_food, food)
        );
    }

    #[test]
    fn test_collision_not_at_zero() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins);
        app.add_plugins(shared::collision::CollisionPlugin);
        app.add_plugins(FoodPlugin);
        // snake: vertical, pointing up
        let snake = app.world_mut().spawn(SnakeBundle::default()).id();
        app.world_mut().entity_mut(snake).insert(SnakeHead {
            position: Vec2::new(0.0, 200.0),
            direction: Direction::Up,
        });
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
            vec![FoodCollision {
                snake,
                food,
                food_position: Vec2::new(0.0, 201.0),
                head_position: Vec2::new(0.0, 200.0),
            }]
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
            vec![FoodCollision {
                snake,
                food,
                food_position: Vec2::new(0.0, 1.0),
                head_position: Vec2::ZERO,
            }]
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
