mod album;
mod exif;
mod file_type;
mod index_cmd;
mod markdown;
mod info_cmd;
mod media;
mod supplemental_info;
mod sync_cmd;
mod test_util;
mod util;

use clap::{Parser, Subcommand};
use log::{LevelFilter, debug, error, info};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Info {
        #[arg(short, long, help = "Turn debugging information on")]
        debug: bool,

        #[arg(short, long, help = "The takeout or iCloud zip/directory")]
        root: String,

        #[arg(short, long, help = "Photo, video or album to view info for")]
        input: String,
    },
    Index {
        #[arg(short, long, help = "Turn debugging information on")]
        debug: bool,

        #[arg(short, long, help = "The takeout or iCloud zip/directory")]
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

        #[arg(long, help = "Skip generating markdown files")]
        skip_markdown: bool,

        #[arg(long, help = "Skip inspecting and copying photo and video files")]
        skip_media: bool,

        #[arg(long, help = "Skip inspecting and copying albums")]
        skip_albums: bool,
    },
}

fn main() {
    match go() {
        Ok(_) => {}
        Err(e) => {
            error!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn go() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Info { debug, root, input } => {
            enable_debug(debug);
            info_cmd::main(&input, &root)?
        }
        Commands::Index { debug, input } => {
            enable_debug(debug);
            index_cmd::main(&input)?
        }
        Commands::Sync {
            debug,
            dry_run,
            skip_markdown,
            input,
            output,
            skip_media,
            skip_albums,
        } => {
            enable_debug(debug);
            enable_dry_run(dry_run);
            sync_cmd::main(
                dry_run,
                &input,
                &output,
                skip_markdown,
                skip_media,
                skip_albums,
            )?;
        }
    }
    Ok(())
}

fn enable_debug(debug: bool) {
    let mut level = LevelFilter::Info;
    if debug {
        level = LevelFilter::Debug;
    }
    env_logger::builder()
        .filter_level(level)
        .format_target(false)
        .format_timestamp(None)
        .format_level(false)
        .init();
    if debug {
        debug!("Debug mode is on");
    }
}

fn enable_dry_run(dry_run: bool) {
    if dry_run {
        info!("Dry run mode is on, no changes will be made to disk");
    }
}
