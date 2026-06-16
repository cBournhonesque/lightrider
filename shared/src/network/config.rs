use std::time::Duration;

use lightyear::netcode::Key;
use lightyear::prelude::{CompressionConfig, LinkConditionerConfig, RecvLinkConditioner};

use crate::config::{NetworkCompression, NetworkConfig};

pub const DEFAULT_PROTOCOL_ID: u64 = 0;
pub const DEFAULT_PRIVATE_KEY: Key = [0; 32];

pub const NETWORK_CONDITIONER_ENV: &str = "LIGHTRIDER_NETWORK_CONDITIONER";

const PROTOCOL_ID_ENV: &str = "LIGHTRIDER_PROTOCOL_ID";
const PRIVATE_KEY_ENV: &str = "LIGHTRIDER_PRIVATE_KEY";
const LEGACY_PRIVATE_KEY_ENV: &str = "LIGHTRIDER_NETCODE_KEY";
const REQUIRE_PRODUCTION_ENV: &str = "LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetcodeIdentity {
    pub protocol_id: u64,
    pub private_key: Key,
}

impl NetcodeIdentity {
    pub fn from_env_or_dev_defaults() -> Self {
        let protocol_id = std::env::var(PROTOCOL_ID_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value
                    .trim()
                    .parse::<u64>()
                    .unwrap_or_else(|error| panic!("invalid {PROTOCOL_ID_ENV}: {error}"))
            })
            .unwrap_or(DEFAULT_PROTOCOL_ID);

        let private_key = std::env::var(PRIVATE_KEY_ENV)
            .ok()
            .or_else(|| std::env::var(LEGACY_PRIVATE_KEY_ENV).ok())
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_private_key(&value))
            .unwrap_or(DEFAULT_PRIVATE_KEY);

        let identity = Self {
            protocol_id,
            private_key,
        };

        if production_required() && identity.is_dev_default() {
            panic!(
                "{REQUIRE_PRODUCTION_ENV}=1 requires nonzero {PROTOCOL_ID_ENV} and {PRIVATE_KEY_ENV}"
            );
        }

        identity
    }

    pub fn is_dev_default(&self) -> bool {
        self.protocol_id == DEFAULT_PROTOCOL_ID || self.private_key == DEFAULT_PRIVATE_KEY
    }
}

pub fn recv_link_conditioner(config: &NetworkConfig) -> Option<RecvLinkConditioner> {
    recv_link_conditioner_config(config).map(RecvLinkConditioner::new)
}

pub fn transport_compression(config: &NetworkConfig) -> CompressionConfig {
    match config.compression {
        NetworkCompression::Disabled => CompressionConfig::DISABLED,
        NetworkCompression::Lz4 => CompressionConfig::LZ4,
    }
}

pub fn recv_link_conditioner_config(config: &NetworkConfig) -> Option<LinkConditionerConfig> {
    if let Some(preset) = std::env::var(NETWORK_CONDITIONER_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        return match preset.as_str() {
            "none" | "off" | "false" | "0" => None,
            "good" => Some(LinkConditionerConfig::good_condition().half()),
            "average" => Some(LinkConditionerConfig::average_condition().half()),
            "poor" => Some(LinkConditionerConfig::poor_condition().half()),
            other => panic!(
                "invalid {NETWORK_CONDITIONER_ENV} '{other}'; expected none, good, average, or poor"
            ),
        };
    }

    if config.artificial_latency_ms == 0
        && config.artificial_jitter_ms == 0
        && config.artificial_loss_percent == 0
    {
        return None;
    }

    Some(LinkConditionerConfig::new(
        Duration::from_millis(config.artificial_latency_ms),
        Duration::from_millis(config.artificial_jitter_ms),
        f32::from(config.artificial_loss_percent.min(100)) / 100.0,
    ))
}

pub fn parse_private_key(value: &str) -> Key {
    let trimmed = value.trim();
    if trimmed.contains(',') {
        return parse_comma_private_key(trimmed);
    }
    parse_hex_private_key(trimmed)
}

fn production_required() -> bool {
    std::env::var(REQUIRE_PRODUCTION_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn parse_comma_private_key(value: &str) -> Key {
    let bytes: Vec<u8> = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<u8>()
                .unwrap_or_else(|error| panic!("invalid private-key byte '{part}': {error}"))
        })
        .collect();
    key_from_vec(bytes)
}

fn parse_hex_private_key(value: &str) -> Key {
    let normalized = value.strip_prefix("0x").unwrap_or(value);
    if normalized.len() != 64 {
        panic!("hex private key must contain exactly 64 hex characters");
    }
    let mut bytes = [0_u8; 32];
    for (index, chunk) in normalized.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(chunk).expect("hex key was not utf8");
        bytes[index] =
            u8::from_str_radix(hex, 16).unwrap_or_else(|error| panic!("invalid hex key: {error}"));
    }
    bytes
}

fn key_from_vec(bytes: Vec<u8>) -> Key {
    if bytes.len() != 32 {
        panic!("private key must contain exactly 32 bytes");
    }
    let mut key = [0_u8; 32];
    key.copy_from_slice(&bytes);
    key
}

#[cfg(test)]
mod tests {
    use crate::config::{InputDelayConfig, LagCompensationConfig, NetworkConfig};

    use super::{parse_private_key, recv_link_conditioner_config};

    #[test]
    fn parses_comma_private_key() {
        let key = parse_private_key(
            "0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31",
        );
        assert_eq!(key[0], 0);
        assert_eq!(key[31], 31);
    }

    #[test]
    fn parses_hex_private_key() {
        let key =
            parse_private_key("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        assert_eq!(key[0], 0);
        assert_eq!(key[31], 31);
    }

    #[test]
    fn config_network_conditioner_uses_one_way_artificial_values() {
        let config = NetworkConfig {
            input_delay: InputDelayConfig::balanced(),
            interpolation_delay: crate::config::InterpolationDelayConfig::default(),
            lag_compensation: LagCompensationConfig::default(),
            interest: crate::config::NetworkInterestConfig::default(),
            input_packet_redundancy_ticks: 3,
            replication_send_hz: 16,
            compression: crate::config::NetworkCompression::Disabled,
            server_port: 5000,
            artificial_latency_ms: 12,
            artificial_jitter_ms: 3,
            artificial_loss_percent: 4,
        };

        let conditioner = recv_link_conditioner_config(&config).unwrap();

        assert_eq!(conditioner.incoming_latency.as_millis(), 12);
        assert_eq!(conditioner.incoming_jitter.as_millis(), 3);
        assert!((conditioner.incoming_loss - 0.04).abs() < f32::EPSILON);
    }
}
