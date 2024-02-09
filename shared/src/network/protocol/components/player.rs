use bevy::prelude::{Component, Entity, Reflect};
use lightyear::prelude::{ClientId, EntityMapper, MapEntities, Message};
use serde::{Deserialize, Serialize};

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq, Reflect)]
#[message(custom_map)]
pub struct Player{
    pub id: ClientId,
    pub name: String,
    pub snake: Option<Entity>,
}

impl<'a> MapEntities<'a> for Player {
    fn map_entities(&mut self, entity_mapper: Box<dyn EntityMapper + 'a>) {
        self.snake.map(|mut x| x.map_entities(entity_mapper));
    }

    fn entities(&self) -> bevy::utils::EntityHashSet<Entity> {
        self.snake.map_or(bevy::utils::EntityHashSet::default(), |x| x.entities())
    }
}

