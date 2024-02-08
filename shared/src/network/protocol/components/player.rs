use bevy::prelude::Component;
use lightyear::prelude::{ClientId, Message};
use serde::{Deserialize, Serialize};

#[derive(Component, Message, Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Player{
    pub id: ClientId,
    pub name: String,
}