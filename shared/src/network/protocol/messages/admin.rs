use bevy::prelude::Message;
use serde::{Deserialize, Serialize};

use crate::network::protocol::components::common::RoomId;

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AdminLoginRequest {
    pub password: String,
}

#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminCommand {
    SetNumBots { count: u16 },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdminStatus {
    pub authenticated: bool,
    pub room: Option<RoomId>,
    pub bot_count: u16,
    pub target_bot_count: u16,
}

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AdminResponse {
    pub accepted: bool,
    pub message: String,
    pub status: AdminStatus,
}
