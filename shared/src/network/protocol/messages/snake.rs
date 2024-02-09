use bevy::prelude::Event;
use lightyear::prelude::{ClientId, Message};
use serde::{Deserialize, Serialize};

#[derive(Message, Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SnakeCollision {
    pub killer: ClientId,
    pub killed: ClientId,
}