mod album;
mod exif_util;
mod markdown_cmd;
mod media_file;
mod sync_cmd;
mod takeout_reader;
mod test_util;
mod upload;
mod util;

use clap::{Parser, Subcommand};
use tracing::{error, info};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Turn debugging information on
    #[arg(short, long)]
    debug: bool,

    /// If set, don't do anything, just print what would be done.
    #[arg(short = 'n', long)]
    dry_run: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Markdown {
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

async fn go() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut tracing_level = tracing::Level::INFO;
    if cli.debug {
        tracing_level = tracing::Level::DEBUG;
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing_level)
        // disable printing the name of the module in every log line.
        .with_target(false)
        .init();
    if cli.debug {
        info!("Debug mode is on");
    }
    if cli.dry_run {
        info!("Dry run mode is on");
    }

    match cli.command {
        Commands::Markdown { input, output } => markdown_cmd::main(&input, &output, &cli.debug, &cli.dry_run)?,
        Commands::Sync {
            directory,
            input_takeout,
            input_icloud,
        } => {
            sync_cmd::main(
                &directory,
                &input_takeout,
                &input_icloud,
                &cli.debug,
                &cli.dry_run,
            )?;
        }
    }

    Ok(())
}
