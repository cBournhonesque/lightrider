use clap::Parser;

use server::app;
use server::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut app = app(cli).await;
    app.run();
}