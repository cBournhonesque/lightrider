use lightyear::prelude::component_protocol;
use leafwing_input_manager::prelude::ActionState;

pub mod snake;
pub mod player;

use super::GameProtocol;


#[component_protocol(protocol = GameProtocol)]
pub enum Components {
    // snake
    TailLength(snake::TailLength),
    HeadPoint(snake::HeadPoint),
    TailPoints(snake::TailPoints),
    Speed(snake::Speed),
    Acceleration(snake::Acceleration),
    // player
    Player(player::Player),
}


