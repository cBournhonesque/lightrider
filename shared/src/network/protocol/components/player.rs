use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::{Component, Entity, Reflect};
use lightyear::prelude::PeerId;
use serde::{Deserialize, Serialize};

#[derive(Component, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
#[component(map_entities)]
pub struct Player {
    pub id: PeerId,
    pub name: String,
    pub snake: Option<Entity>,
}

impl MapEntities for Player {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.snake = self.snake.map(|x| entity_mapper.get_mapped(x));
    }
}

#[derive(Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerScore {
    pub value: u32,
}

impl PlayerScore {
    pub fn from_length(length: f32) -> Self {
        Self {
            value: length.max(0.0).round() as u32,
        }
    }

    pub fn from_tail_length(tail_length: f32, starting_tail_length: f32) -> Self {
        Self::from_length(tail_length - starting_tail_length)
    }
}

#[derive(Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerStats {
    pub average_speed: f32,
    pub speed_samples: u32,
    pub time_alive_seconds: f32,
    pub kills: u32,
    pub time_as_leader_seconds: f32,
    pub food_eaten: u32,
}

impl PlayerStats {
    pub fn reset_for_life(&mut self) {
        *self = Self::default();
    }

    pub fn record_speed(&mut self, speed: f32) {
        let speed = speed.max(0.0);
        self.speed_samples = self.speed_samples.saturating_add(1);
        let samples = self.speed_samples as f32;
        self.average_speed += (speed - self.average_speed) / samples;
    }
}

#[derive(
    Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect,
)]
pub struct PlayerRank {
    /// One-based rank within the player's current room. `0` means unranked/not computed yet.
    pub value: u16,
}

#[derive(
    Component, Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect,
)]
pub enum PlayerStatus {
    #[default]
    Alive,
    Dead,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_stats_average_speed_is_incremental() {
        let mut stats = PlayerStats::default();

        stats.record_speed(1.0);
        stats.record_speed(3.0);

        assert_eq!(stats.speed_samples, 2);
        assert_eq!(stats.average_speed, 2.0);
    }

    #[test]
    fn player_stats_reset_clears_life_values() {
        let mut stats = PlayerStats {
            average_speed: 2.0,
            speed_samples: 3,
            time_alive_seconds: 4.0,
            kills: 5,
            time_as_leader_seconds: 6.0,
            food_eaten: 7,
        };

        stats.reset_for_life();

        assert_eq!(stats, PlayerStats::default());
    }

    #[test]
    fn player_score_ignores_starting_tail_length() {
        assert_eq!(PlayerScore::from_tail_length(200.0, 200.0).value, 0);
        assert_eq!(PlayerScore::from_tail_length(220.0, 200.0).value, 20);
        assert_eq!(PlayerScore::from_tail_length(180.0, 200.0).value, 0);
    }

    #[test]
    fn player_component_maps_snake_entity() {
        let server_snake = Entity::from_bits(1);
        let client_snake = Entity::from_bits(2);
        let mut player = Player {
            id: PeerId::Netcode(7),
            name: "local".to_string(),
            snake: Some(server_snake),
        };

        Component::map_entities(&mut player, &mut (server_snake, client_snake));

        assert_eq!(player.snake, Some(client_snake));
    }
}
