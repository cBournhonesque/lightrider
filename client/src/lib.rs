use std::net::Ipv4Addr;
use bevy::app::App;
use bevy::DefaultPlugins;
use clap::Parser;
use shared::network::config::Transports;

mod network;


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

pub fn app(cli: Cli) {
    let mut app = App::new();
    app.add_plugin(DefaultPlugins);

    // networking
    app.add_plugin(network::config::build_plugin(
        cli.client_id,
        cli.client_port,
        (cli.server_addr, cli.server_port).into(),
        cli.transport,
    ));
}