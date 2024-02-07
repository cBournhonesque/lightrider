use lightyear::prelude::component_protocol;
use leafwing_input_manager::prelude::ActionState;

pub mod snake;
pub mod player;

use super::GameProtocol;


#[component_protocol(protocol = GameProtocol)]
pub enum Components {
    // snake
    // tail
    TailPoints(snake::TailPoints),
    TailParent(snake::TailParent),
    // head
    TailLength(snake::TailLength),
    HeadPoint(snake::HeadPoint),
    Speed(snake::Speed),
    Acceleration(snake::Acceleration),
    // player
    Player(player::Player),
}


