use bevy::prelude::Message;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::network::protocol::components::common::RoomId;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RoomCode([u8; 4]);

impl RoomCode {
    pub const ROOM_ID_PREFIX: u64 = 1_u64 << 62;

    pub fn parse(value: &str) -> Result<Self, String> {
        let trimmed = value.trim();
        let mut bytes = [0_u8; 4];
        let mut count = 0;
        for (index, character) in trimmed.chars().enumerate() {
            if index >= bytes.len() {
                return Err("private room codes must be exactly four letters".to_string());
            }
            let upper = character.to_ascii_uppercase();
            if !upper.is_ascii_uppercase() {
                return Err("private room codes must use letters A-Z".to_string());
            }
            bytes[index] = upper as u8;
            count += 1;
        }
        if count != bytes.len() {
            return Err("private room codes must be exactly four letters".to_string());
        }
        Ok(Self(bytes))
    }

    pub fn room_id(self) -> RoomId {
        let mut index = 0_u64;
        for byte in self.0 {
            index = index * 26 + u64::from(byte - b'A');
        }
        RoomId(Self::ROOM_ID_PREFIX | index)
    }

    pub fn from_room_id(room_id: RoomId) -> Option<Self> {
        if !Self::is_private_room_id(room_id) {
            return None;
        }
        let mut index = room_id.0 & !Self::ROOM_ID_PREFIX;
        if index >= 26_u64.pow(4) {
            return None;
        }
        let mut bytes = [b'A'; 4];
        for byte in bytes.iter_mut().rev() {
            *byte = b'A' + (index % 26) as u8;
            index /= 26;
        }
        Some(Self(bytes))
    }

    pub fn is_private_room_id(room_id: RoomId) -> bool {
        room_id.0 & Self::ROOM_ID_PREFIX != 0
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("room codes are validated as ASCII letters")
    }
}

impl fmt::Display for RoomCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomJoinMode {
    Auto,
    New,
    Specific(RoomId),
    Private(RoomCode),
}

#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomJoinRequest {
    pub mode: RoomJoinMode,
}

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlayerNameUpdate {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_codes_are_normalized_to_uppercase() {
        let code = RoomCode::parse("abcz").unwrap();
        assert_eq!(code.as_str(), "ABCZ");
        assert_eq!(code.to_string(), "ABCZ");
    }

    #[test]
    fn room_codes_reject_non_four_letter_values() {
        assert!(RoomCode::parse("ABC").is_err());
        assert!(RoomCode::parse("ABCDE").is_err());
        assert!(RoomCode::parse("AB1D").is_err());
    }

    #[test]
    fn room_codes_map_to_stable_private_room_ids() {
        assert_eq!(
            RoomCode::parse("AAAA").unwrap().room_id(),
            RoomId(RoomCode::ROOM_ID_PREFIX)
        );
        assert_eq!(
            RoomCode::parse("AAAB").unwrap().room_id(),
            RoomId(RoomCode::ROOM_ID_PREFIX + 1)
        );
        assert_eq!(
            RoomCode::parse("AABA").unwrap().room_id(),
            RoomId(RoomCode::ROOM_ID_PREFIX + 26)
        );
    }

    #[test]
    fn room_codes_round_trip_from_room_ids() {
        for value in ["AAAA", "ABCD", "WXYZ", "ZZZZ"] {
            let code = RoomCode::parse(value).unwrap();
            assert_eq!(RoomCode::from_room_id(code.room_id()), Some(code));
        }
        assert_eq!(RoomCode::from_room_id(RoomId(42)), None);
    }
}
