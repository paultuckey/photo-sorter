mod util;
mod album;
mod takeout_reader;
mod tui;
mod exif_util;
mod test_util;
mod markdown_cmd;
mod sync_cmd;

use clap::{Parser, Subcommand};
use std::fs;
use std::time::Duration;
use anyhow::{Context};
use tracing::{error, info};
use crate::markdown_cmd::show_exif;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Markdown {
        #[arg(
            short,
            long,
            help = "Photo or video to generate markdown for"
        )]
        input: String,
        
        #[arg(
            short,
            long,
            help = "Markdown file to output to"
        )]
        output: Option<String>,
    },
    Sort {
        #[arg(
            short,
            long,
            help = "Directory to sync photos into"
        )]
        directory: String,

        #[arg(long)]
        input_takeout: Option<String>,

        #[arg(long)]
        input_icloud: Option<String>,

        #[arg(long)]
        output: Option<String>,

    },
}


#[tokio::main]
async fn main() {
    match go().await {
        Ok(_) => {}
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn go() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    let mut tracing_level = tracing::Level::INFO;
    match cli.debug {
        0 => info!("Debug mode is off"),
        _ => {
            info!("Debug mode is on");
            tracing_level = tracing::Level::DEBUG;
        }
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing_level)
        // disable printing the name of the module in every log line.
        .with_target(false)
        .init();

    match cli.command {
        Commands::Markdown { input, output } => {
            show_exif(&input, Option::from(&output)).unwrap()
        }
        Commands::Sort {  directory, input_takeout, input_icloud, output } => {
            todo!();
        }
    }
    
    Ok(())
}
