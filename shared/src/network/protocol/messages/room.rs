use bevy::prelude::Message;
use serde::{Deserialize, Serialize};

use crate::network::protocol::components::common::RoomId;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomJoinMode {
    Auto,
    New,
    Specific(RoomId),
}

#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomJoinRequest {
    pub mode: RoomJoinMode,
}
