use lightyear::prelude::*;

use super::GameProtocol;

pub(crate) mod snake;

#[message_protocol(protocol = GameProtocol)]
pub enum Messages {
    SnakeCollision(snake::SnakeCollision),
}