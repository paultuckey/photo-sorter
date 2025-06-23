mod album;
mod exif;
mod extra_info;
mod file_type;
mod markdown_cmd;
mod media;
mod sync_cmd;
mod test_util;
mod util;

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
        #[arg(short, long, help = "Turn debugging information on")]
        debug: bool,

        #[arg(short, long, help = "Photo or video to generate markdown for")]
        input: String,
    },
    Sync {
        #[arg(short, long, help = "Turn debugging information on")]
        debug: bool,

        #[arg(
            short = 'n',
            long,
            help = "If set, don't do anything, just print what would be done."
        )]
        dry_run: bool,

        #[arg(long, help = "Google Takeout or iCloud input directory or zip file")]
        input: String,

        #[arg(short, long, help = "Directory to sync photos and videos into")]
        output: Option<String>,

        #[arg(short, long, help = "Skip generating markdown files")]
        skip_markdown: bool,
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

fn enable_debug(debug: bool) {
    let mut tracing_level = tracing::Level::INFO;
    if debug {
        tracing_level = tracing::Level::DEBUG;
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing_level)
        // disable printing the name of the module in every log line.
        .with_target(false)
        .init();
    if debug {
        info!("Debug mode is on");
    }
}

fn enable_dry_run(dry_run: bool) {
    if dry_run {
        info!("Dry run mode is on, no changes will be made to disk");
    }
}

async fn go() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Markdown { debug, input } => {
            enable_debug(debug);
            markdown_cmd::main(&input)?
        }
        Commands::Sync {
            debug,
            dry_run,
            skip_markdown,
            input,
            output,
        } => {
            enable_debug(debug);
            enable_dry_run(dry_run);
            sync_cmd::main(debug, dry_run, &input, &output, skip_markdown).await?;
        }
    }

    Ok(())
}
