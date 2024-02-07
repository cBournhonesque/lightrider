use lightyear::prelude::*;
use serde::{Deserialize, Serialize};
use super::GameProtocol;

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message1(pub usize);

#[message_protocol(protocol = GameProtocol)]
pub enum Messages {
    Message1(Message1),
}