use clap::Parser;
use server::Cli;
use server::app;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut app = app(cli).await;
    app.run();
}