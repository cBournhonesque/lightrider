use lightyear::prelude::component_protocol;
use leafwing_input_manager::prelude::ActionState;

pub mod snake;
pub mod player;

use super::GameProtocol;


#[component_protocol(protocol = GameProtocol)]
pub enum Components {
    // snake
    // tail
    #[sync(full, lerp = "NullInterpolator")]
    TailPoints(snake::TailPoints),
    #[sync(once)]
    TailParent(snake::TailParent),
    // head
    #[sync(full, lerp = "NullInterpolator")]
    TailLength(snake::TailLength),
    #[sync(full, lerp = "NullInterpolator")]
    HeadPoint(snake::HeadPoint),
    #[sync(full, lerp = "NullInterpolator")]
    HeadDirection(snake::HeadDirection),
    #[sync(full, lerp = "NullInterpolator")]
    Speed(snake::Speed),
    #[sync(full, lerp = "NullInterpolator")]
    Acceleration(snake::Acceleration),
    // player
    #[sync(once)]
    Player(player::Player),
}


