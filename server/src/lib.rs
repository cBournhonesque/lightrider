use bevy::app::App;
use bevy::DefaultPlugins;
use clap::Parser;
use shared::movement::MovementPlugin;
use shared::network::config::Transports;
use shared::SharedPlugin;

mod network;
mod debug;


#[derive(Parser, PartialEq, Debug)]
pub struct Cli {
    #[arg(long, default_value = "false")]
    headless: bool,

    #[arg(short, long, default_value = "false")]
    inspector: bool,

    #[arg(short, long, default_value_t = SERVER_PORT)]
    port: u16,

    #[arg(short, long, value_enum, default_value_t = Transports::WebTransport)]
    transport: Transports,
}


pub async fn app(cli: Cli) -> App {
    let mut app = App::new();
    app.add_plugin(DefaultPlugins);

    // networking
    app.add_plugins(network::build_plugin(cli.port, cli.transport).await);

    // shared
    app.add_plugins(SharedPlugin);
    app
}