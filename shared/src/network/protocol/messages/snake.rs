use lightyear::prelude::{ClientId, Message};
use serde::{Deserialize, Serialize};


#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Collision {
    pub killer: ClientId,
    pub killed: ClientId,
}