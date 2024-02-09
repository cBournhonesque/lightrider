use lightyear::prelude::component_protocol;
// TODO: why is this import needed?
use leafwing_input_manager::prelude::ActionState;

use super::GameProtocol;

pub mod snake;
pub mod player;

#[component_protocol(protocol = GameProtocol)]
pub enum Components {
    // snake
    #[sync(full, lerp = "NullInterpolator")]
    TailPoints(snake::TailPoints),
    #[sync(full, lerp = "NullInterpolator")]
    TailLength(snake::TailLength),
    #[sync(full, lerp = "NullInterpolator")]
    Speed(snake::Speed),
    #[sync(full, lerp = "NullInterpolator")]
    Acceleration(snake::Acceleration),
    #[sync(once)]
    HasPlayer(snake::HasPlayer),
    // player
    #[sync(simple)]
    Player(player::Player),
}


