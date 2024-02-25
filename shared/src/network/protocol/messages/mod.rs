use lightyear::prelude::*;

use super::GameProtocol;

pub(crate) mod snake;
pub(crate) mod food;

#[message_protocol(protocol = GameProtocol)]
pub enum Messages {
    SnakeCollision(snake::SnakeCollision),
    FoodCollision(food::FoodCollision),
}