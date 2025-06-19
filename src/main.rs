mod album;
mod exif_util;
mod markdown_cmd;
mod media_file;
mod sync_cmd;
mod takeout_reader;
mod test_util;
mod upload;
mod util;
mod extra_info;

use clap::{Parser, Subcommand};
use tracing::{error, info};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Markdown {
        /// Turn debugging information on
        #[arg(short, long)]
        debug: bool,

        /// If set, don't do anything, just print what would be done.
        #[arg(short = 'n', long)]
        dry_run: bool,

        #[arg(short, long, help = "Photo or video to generate markdown for")]
        input: String,

        #[arg(
            short,
            long,
            help = "Markdown file to output to, console output if not specified"
        )]
        output: Option<String>,
    },
    Sync {
        /// Turn debugging information on
        #[arg(short, long)]
        debug: bool,

        /// If set, don't do anything, just print what would be done.
        #[arg(short = 'n', long)]
        dry_run: bool,

        #[arg(short, long, help = "Directory to sync photos into")]
        directory: Option<String>,

        #[arg(long)]
        input_takeout: Option<String>,

        #[arg(long)]
        input_icloud: Option<String>,
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

fn enable_debug(debug: &bool) {
    let mut tracing_level = tracing::Level::INFO;
    if debug.clone() {
        tracing_level = tracing::Level::DEBUG;
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing_level)
        // disable printing the name of the module in every log line.
        .with_target(false)
        .init();
    if debug.clone() {
        info!("Debug mode is on");
    }
}

fn enable_dry_run(dry_run: &bool) {
    if dry_run.clone() {
        info!("Dry run mode is on, no changes will be made to disk");
    }
}

async fn go() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Markdown { debug, dry_run, input, output } => {
            enable_debug(&debug);
            enable_dry_run(&dry_run);
            markdown_cmd::main(&input, &output, &debug, &dry_run)?
        },
        Commands::Sync {
            debug, dry_run,
            directory,
            input_takeout,
            input_icloud,
        } => {
            enable_debug(&debug);
            enable_dry_run(&dry_run);
            sync_cmd::main(
                &directory,
                &input_takeout,
                &input_icloud,
                &debug,
                &dry_run,
            ).await?;
        }
    }

    Ok(())
}
