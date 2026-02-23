mod album;
mod db_cmd;
mod exif_util;
mod file_type;
mod fs;
mod index_cmd;
mod info_cmd;
mod markdown;
mod media;
mod progress;
mod supplemental_info;
mod sync_cmd;
mod test_util;
mod track_util;
mod util;

use clap::{Parser, Subcommand};
use tracing::{Level, debug, error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show info for an individual photo or video
    Info {
        /// Turn debugging information on
        #[arg(short, long)]
        debug: bool,

        /// The takeout or iCloud zip/directory
        #[arg(short, long)]
        root: String,

        /// Photo, video or album to view info for
        #[arg(short, long)]
        input: String,
    },
    /// Scan files in an archive or directory and output known patterns
    Index {
        /// Turn debugging information on
        #[arg(short, long)]
        debug: bool,

        /// The takeout or iCloud zip/directory
        #[arg(short, long)]
        input: String,
    },
    /// Scan files in an archive or directory and collect meta info into a sqlite database
    Db {
        /// Turn debugging information on
        #[arg(short, long)]
        debug: bool,

        /// The takeout or iCloud zip/directory
        #[arg(short, long)]
        input: String,
    },
    /// Sync files in an archive or directory into a standardised directory structure
    Sync {
        /// Turn debugging information on
        #[arg(short, long)]
        debug: bool,

        /// If set, don't do anything, just print what would be done.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Google Takeout or iCloud input directory or zip file
        #[arg(long)]
        input: String,

        /// Directory to sync photos and videos into
        #[arg(short, long)]
        output: Option<String>,

        /// Skip generating markdown files
        #[arg(long)]
        skip_markdown: bool,

        /// Skip inspecting and copying photo and video files
        #[arg(long)]
        skip_media: bool,

        /// Skip inspecting and copying albums
        #[arg(long)]
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
        Commands::Db { debug, input } => {
            enable_debug(debug);
            db_cmd::main(&input)?
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

struct IndicatifWriter;

impl std::io::Write for IndicatifWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        progress::get_multi_progress().suspend(|| std::io::stderr().write(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stderr().flush()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for IndicatifWriter {
    type Writer = IndicatifWriter;

    fn make_writer(&'a self) -> Self::Writer {
        IndicatifWriter
    }
}

fn enable_debug(debug: bool) {
    let filter = tracing_subscriber::filter::Targets::new()
        .with_default(if debug { Level::DEBUG } else { Level::INFO })
        .with_target("nom_exif", Level::ERROR);
    let registry_layer = tracing_subscriber::fmt::layer()
        .with_writer(IndicatifWriter)
        .with_target(false);
    tracing_subscriber::registry()
        .with(registry_layer)
        .with(filter)
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
