use clap::{Parser, Subcommand};

use crate::{receiver, sender};

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "sync")]
    Sync {
        #[arg(long, short, help = "Directory to sync")]
        from: String,

        #[arg(long, short, help = "Listener address")]
        to: String,

        #[arg(
            long, short, help = "Watch for changes",
            default_value_t = false, action = clap::ArgAction::SetTrue
        )]
        watch: bool,
    },

    #[command(name = "listen")]
    Listen {
        #[arg(long, short, help = "Port to listen on")]
        port: u32,

        #[arg(long, short, help = "Output directory path")]
        output_dir: String,
    },
}

impl Cli {
    pub async fn run(&self) {
        match &self.command {
            Commands::Sync { from, to, watch } => {
                let sender = sender::Sender::new(from, to.as_str());
                sender.start(*watch).await.unwrap();
            }
            Commands::Listen { port, output_dir } => {
                let receiver = receiver::Receiver::new(*port, output_dir);
                receiver.start().await.unwrap();
            }
        }
    }
}
