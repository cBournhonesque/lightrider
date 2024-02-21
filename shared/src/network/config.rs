use std::time::Duration;

use clap::ValueEnum;
use lightyear::prelude::*;

pub const PROTOCOL_ID: u64 = 0;
pub const KEY: Key = [0; 32];

pub const FIXED_TIMESTEP_HZ: f64 = 64.0;
pub const SERVER_SEND_HZ: f64 = 32.0;

pub fn shared_config() -> SharedConfig {
    SharedConfig {
        enable_replication: true,
        client_send_interval: Duration::default(),
        server_send_interval: Duration::from_secs_f64(1.0 / SERVER_SEND_HZ),
        tick: TickConfig {
            tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
        },
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Transports {
    #[cfg(not(target_family = "wasm"))]
    Udp,
    WebTransport,
    WebSocket,
}