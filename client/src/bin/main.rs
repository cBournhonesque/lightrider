use clap::Parser;
use client::{app, Cli};


#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut app = app(cli);
    app.run();
}