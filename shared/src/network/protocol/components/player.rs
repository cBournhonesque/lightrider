use bevy::prelude::Component;
use lightyear::prelude::Message;
use serde::{Deserialize, Serialize};

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Player{
    pub id: u64,
}