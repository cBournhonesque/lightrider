mod snake;

use lightyear::prelude::*;
use super::GameProtocol;


#[message_protocol(protocol = GameProtocol)]
pub enum Messages {
    Collision(snake::Collision),
}