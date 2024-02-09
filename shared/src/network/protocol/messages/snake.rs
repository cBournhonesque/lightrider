use bevy::prelude::{Entity, Event};
use lightyear::prelude::{EntityMapper, MapEntities, Message};
use serde::{Deserialize, Serialize};

#[derive(Message, Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[message(custom_map)]
pub struct SnakeCollision {
    pub killer: Entity,
    pub killed: Entity,
}

impl<'a> MapEntities<'a> for SnakeCollision {
    // TODO: we cannot use map_entities(entity_mapper) twice! Need to rework the trait
    fn map_entities(&mut self, entity_mapper: Box<dyn EntityMapper + 'a>) {
        if let Some(e) = entity_mapper.map(self.killer) {
            self.killer = e;
        }
        self.killed.map_entities(entity_mapper);
    }

    fn entities(&self) -> bevy::utils::EntityHashSet<Entity> {

        bevy::utils::EntityHashSet::from_iter(vec![self.killer, self.killed])
    }
}