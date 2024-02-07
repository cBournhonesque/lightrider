use std::net::Ipv4Addr;
use bevy::app::App;
use bevy::DefaultPlugins;
use clap::Parser;
use shared::network::config::Transports;
use shared::SharedPlugin;

mod network;
mod render;

// Use a port of 0 to automatically select a port
pub const CLIENT_PORT: u16 = 0;

pub const SERVER_PORT: u16 = 5000;


#[derive(Parser, PartialEq, Debug)]
pub struct Cli {
    #[arg(short, long, default_value = "false")]
    inspector: bool,

    #[arg(short, long, default_value_t = 0)]
    client_id: u64,

    #[arg(long, default_value_t = CLIENT_PORT)]
    client_port: u16,

    #[arg(long, default_value_t = Ipv4Addr::LOCALHOST)]
    server_addr: Ipv4Addr,

    #[arg(short, long, default_value_t = SERVER_PORT)]
    server_port: u16,

    #[arg(short, long, value_enum, default_value_t = Transports::WebTransport)]
    transport: Transports,
}

pub fn app(cli: Cli) -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);

    // networking
    app.add_plugins(network::NetworkPlugin {
        client_id: cli.client_id,
        client_port: cli.client_port,
        server_addr: (cli.server_addr, cli.server_port).into(),
        transport: cli.transport,
    });

    // render
    app.add_plugins(render::RenderPlugin);

    // shared
    app.add_plugins(SharedPlugin);
    app
}