use clap::Parser;

mod cli;
mod core;
mod receiver;
mod sender;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();
    cli.run().await;
}
