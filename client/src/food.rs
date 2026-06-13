use std::collections::VecDeque;

use bevy::prelude::*;
use lightyear::prelude::{
    Client, LocalTimeline, MessageReceiver, Predicted, PredictionAppRegistrationExt,
};

use shared::collision::collider::ColliderSet;
use shared::config::GameConfig;
use shared::movement::SimulationSet;
use shared::network::protocol::prelude::*;
use shared::spatial::{FoodPoint, FoodSpatialIndex};

pub(crate) struct PredictedFoodPlugin;

#[derive(Message, Clone, Debug, PartialEq)]
pub(crate) struct ConfirmedFoodPickup {
    pub(crate) collision: FoodCollision,
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct RecentlyBoostedFromFood {
    foods: VecDeque<Entity>,
}

impl Plugin for PredictedFoodPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConfirmedFoodPickup>();
        app.add_rollback::<RecentlyBoostedFromFood>();
        app.add_systems(Update, add_recently_boosted_from_food_history);
        app.add_systems(Update, receive_confirmed_food_pickups);
        app.add_systems(
            FixedUpdate,
            predict_food_boosts
                .in_set(ColliderSet::ComputeCollision)
                .after(SimulationSet::Movement),
        );
    }
}

fn receive_confirmed_food_pickups(
    mut receivers: Query<&mut MessageReceiver<FoodCollision>, With<Client>>,
    mut confirmed_pickups: MessageWriter<ConfirmedFoodPickup>,
) {
    let Ok(mut receiver) = receivers.single_mut() else {
        return;
    };
    for collision in receiver.receive() {
        confirmed_pickups.write(ConfirmedFoodPickup { collision });
    }
}

fn add_recently_boosted_from_food_history(
    mut commands: Commands,
    snakes: Query<
        Entity,
        (
            With<Predicted>,
            With<SnakeHead>,
            Without<RecentlyBoostedFromFood>,
        ),
    >,
) {
    for snake in &snakes {
        commands
            .entity(snake)
            .insert(RecentlyBoostedFromFood::default());
    }
}

fn predict_food_boosts(
    config: Res<GameConfig>,
    timeline: Res<LocalTimeline>,
    food: Query<(Entity, &Position, &RoomId), With<FoodMarker>>,
    mut snakes: Query<
        (
            Entity,
            &SnakeHead,
            &RoomId,
            &mut FoodBoost,
            &mut RecentlyBoostedFromFood,
        ),
        With<Predicted>,
    >,
) {
    let candidates = food
        .iter()
        .map(|(entity, position, room)| FoodPoint {
            entity,
            position: position.0,
            room: *room,
        })
        .collect::<Vec<_>>();
    let food_index = FoodSpatialIndex::from_food(candidates.iter().copied());

    for (snake, head, room, mut food_boost, mut recently_boosted) in &mut snakes {
        recently_boosted.retain_known_food(&candidates);
        let near_food = food_index.within_radius(*room, head.position, config.food.radius);
        if let Some(food) = predict_food_boost_for_snake(
            &config,
            head.position,
            *room,
            &mut food_boost,
            &mut recently_boosted,
            near_food,
        ) {
            trace!(
                target: "lightyear_debug::manual",
                kind = "predicted_food_boost",
                schedule = "FixedUpdate",
                sample_point = "FixedUpdate",
                tick_id = timeline.tick().0,
                snake = ?snake,
                food = ?food,
                prediction_radius = config.food.radius.max(0.0),
                food_boost = food_boost.0,
                "predicted local food boost"
            );
        }
    }
}

fn predict_food_boost_for_snake(
    config: &GameConfig,
    head: Vec2,
    room: RoomId,
    food_boost: &mut FoodBoost,
    recently_boosted: &mut RecentlyBoostedFromFood,
    foods: impl IntoIterator<Item = FoodPoint>,
) -> Option<Entity> {
    let radius = config.food.radius.max(0.0);
    let food_boost_acceleration = config.movement.food_boost_acceleration.max(0.0);
    for food in foods {
        if food.room != room || recently_boosted.contains(food.entity) {
            continue;
        }
        if head.distance(food.position) > radius {
            continue;
        }
        recently_boosted.push(food.entity);
        food_boost.0 += food_boost_acceleration;
        return Some(food.entity);
    }
    None
}

impl RecentlyBoostedFromFood {
    fn retain_known_food(&mut self, candidates: &[FoodPoint]) {
        self.foods
            .retain(|boosted| candidates.iter().any(|food| food.entity == *boosted));
    }

    fn contains(&self, food: Entity) -> bool {
        self.foods.contains(&food)
    }

    fn push(&mut self, food: Entity) {
        if !self.contains(food) {
            self.foods.push_back(food);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicted_food_boost_applies_movement_effect_once() {
        let config = GameConfig::default();
        let food = Entity::from_bits(42);
        let mut recently_boosted = RecentlyBoostedFromFood::default();
        let mut food_boost = FoodBoost::default();
        let candidate = FoodPoint {
            entity: food,
            position: Vec2::new(config.food.radius * 0.5, 0.0),
            room: RoomId(7),
        };

        assert_eq!(
            predict_food_boost_for_snake(
                &config,
                Vec2::ZERO,
                RoomId(7),
                &mut food_boost,
                &mut recently_boosted,
                [candidate],
            ),
            Some(food)
        );
        assert_eq!(
            food_boost,
            FoodBoost(GameConfig::default().movement.food_boost_acceleration)
        );

        assert_eq!(
            predict_food_boost_for_snake(
                &config,
                Vec2::ZERO,
                RoomId(7),
                &mut food_boost,
                &mut recently_boosted,
                [candidate],
            ),
            None
        );
        assert_eq!(
            food_boost,
            FoodBoost(GameConfig::default().movement.food_boost_acceleration)
        );
    }

    #[test]
    fn predicted_food_boost_ignores_other_rooms() {
        let config = GameConfig::default();
        let mut recently_boosted = RecentlyBoostedFromFood::default();
        let mut food_boost = FoodBoost::default();

        assert_eq!(
            predict_food_boost_for_snake(
                &config,
                Vec2::ZERO,
                RoomId(1),
                &mut food_boost,
                &mut recently_boosted,
                [FoodPoint {
                    entity: Entity::from_bits(100),
                    position: Vec2::ZERO,
                    room: RoomId(2),
                }],
            ),
            None
        );
        assert_eq!(food_boost, FoodBoost::default());
    }

    #[test]
    fn predicted_food_boost_uses_server_pickup_radius() {
        let config = GameConfig::default();
        let mut recently_boosted = RecentlyBoostedFromFood::default();
        let mut food_boost = FoodBoost::default();

        assert_eq!(
            predict_food_boost_for_snake(
                &config,
                Vec2::ZERO,
                RoomId(1),
                &mut food_boost,
                &mut recently_boosted,
                [FoodPoint {
                    entity: Entity::from_bits(101),
                    position: Vec2::new(config.food.radius + 0.01, 0.0),
                    room: RoomId(1),
                }],
            ),
            None
        );
        assert_eq!(food_boost, FoodBoost::default());
    }

    #[test]
    fn recently_boosted_from_food_retains_only_visible_food() {
        let kept = Entity::from_bits(1);
        let removed = Entity::from_bits(2);
        let mut recently_boosted = RecentlyBoostedFromFood::default();
        recently_boosted.push(removed);
        recently_boosted.push(kept);

        recently_boosted.retain_known_food(&[FoodPoint {
            entity: kept,
            position: Vec2::ZERO,
            room: RoomId(1),
        }]);

        assert_eq!(recently_boosted.foods, VecDeque::from([kept]));
    }
}
