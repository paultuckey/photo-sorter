use crate::file_type::{QuickFileType, QuickScannedFile, quick_file_scan};
use crate::media::{MediaFileInfo, media_file_info_from_readable};
use crate::util::PsReadableFile;
use crate::zip_reader;
use crate::zip_reader::PsReadableFromZip;
use anyhow::anyhow;
use console::Term;
use futures::StreamExt;
use indicatif::ProgressBar;
use once_cell::sync::Lazy;
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::{fs, thread, time::Duration};
use tracing::debug;

struct State {
    init_spinner: ProgressBar,
    indexing_spinner: ProgressBar,
    media_progress: ProgressBar,
    albums_progress: ProgressBar,
}

static UI: Lazy<State> = Lazy::new(|| State {
    init_spinner: ProgressBar::new_spinner(),
    indexing_spinner: ProgressBar::new_spinner(),
    media_progress: ProgressBar::new(32),
    albums_progress: ProgressBar::new(32),
});

pub(crate) async fn main(
    debug: bool,
    dry_run: bool,
    input: &Option<String>,
    output_directory: &Option<String>,
    skip_markdown: bool,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel();

    let terminal_output_thread = thread::spawn(move || {
        let term = Term::stdout();
        while let Ok(event) = rx.recv() {
            handle(&term, event, debug).unwrap();
        }
    });

    start(tx, dry_run, input, output_directory, skip_markdown).await?;
    terminal_output_thread.join().unwrap();

    Ok(())
}

async fn start(
    tx: Sender<ProgressEvent>,
    dry_run: bool,
    input: &Option<String>,
    output_directory: &Option<String>,
    skip_markdown: bool,
) -> anyhow::Result<()> {
    let input_str = input.clone().unwrap();
    tx.send(ProgressEvent::Start(input_str.clone()))?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let files = zip_reader::scan(&input_str)?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let quick_scanned_files = quick_file_scan(files);

    let media_len = quick_scanned_files
        .iter()
        .filter(|m| m.quick_file_type == QuickFileType::Media)
        .count() as u32;
    tx.send(ProgressEvent::MediaFilesCalculated(media_len))?;

    // scan zip and collect list of files with right extensions and file size
    // then find any extra files for meta info
    // find albums (based on paths)
    // and then analyze each file

    let num_threads = 10; // should be related to number of cored on underlying machine
    let fetches = futures::stream::iter(
        quick_scanned_files //
            .iter()
            .map(|quick_scanned_file| {
                let input_str2 = input_str.clone();
                let tx2 = tx.clone();
                async move {
                    let root = PsReadableFromZip::new(input_str2, "".to_string());
                    let scanned_file = analyze(
                        &root,
                        quick_scanned_file,
                        dry_run,
                        output_directory,
                        skip_markdown,
                    );
                    let name = &quick_scanned_file.name;
                    tx2.send(ProgressEvent::MediaFileDone(name.clone()))
                        .expect("send media file");
                    scanned_file
                }
            }),
    )
    .buffer_unordered(num_threads)
    .collect::<Vec<anyhow::Result<MediaFileInfo>>>();
    fetches.await;

    tx.send(ProgressEvent::MediaDone())?;

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let total_albums = 5;
    tx.send(ProgressEvent::AlbumsCalculated(total_albums))?;
    tokio::time::sleep(Duration::from_millis(1000)).await;

    for _ in 0..total_albums {
        tx.send(ProgressEvent::AlbumFileDone())?;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    tx.send(ProgressEvent::AlbumsDone())?;
    tokio::time::sleep(Duration::from_millis(1000)).await;

    tx.send(ProgressEvent::AllDone())?;
    tokio::time::sleep(Duration::from_millis(1000)).await;
    Ok(())
}

pub(crate) fn analyze(
    root_readable: &dyn PsReadableFile,
    qsf: &QuickScannedFile,
    dry_run: bool,
    output_directory: &Option<String>,
    skip_markdown: bool,
) -> anyhow::Result<MediaFileInfo> {
    let file = &qsf.name;
    debug!("Analyzing {:?}", file);
    let extra_info_path = qsf.supplemental_json_file.clone();
    let f_box = root_readable.another(&file.clone());
    let f = f_box.as_ref();

    let media_file_info_res = media_file_info_from_readable(f, &extra_info_path);
    let Ok(media_file) = media_file_info_res else {
        debug!("File type unsupported: {:?}", file);
        return Err(anyhow!("Unsupported file type: {:?}", file));
    };

    if let Some(output_dir) = output_directory {
        if let Some(d) = media_file.desired_media_path.clone() {
            let output_path = Path::new(output_dir).join(&d);
            // todo: check if output_path exists, if so, use checksum
            if dry_run {
                debug!("Would create media file at {:?}", output_path);
            } else {
                // todo fs::create_dir_all(output_path.parent().unwrap())?;
                // fs::write(&output_path, f.to_bytes()?)?;
                debug!("Created file at {:?}", output_path);
            }
        }
        if !skip_markdown {
            if let Some(m) = media_file.desired_markdown_path.clone() {
                let output_path = Path::new(output_dir).join(&m);
                if dry_run {
                    debug!("Would create markdown file at {:?}", output_path);
                } else {
                    // todo fs::create_dir_all(output_path.parent().unwrap())?;
                    // fs::write(&output_path, mfm_to_string(&media_file))?;
                    debug!("Created markdown file at {:?}", output_path);
                }
            }
        }
    }
    Ok(media_file)
}

fn handle(term: &Term, e: ProgressEvent, debug: bool) -> anyhow::Result<()> {
    match e {
        ProgressEvent::Start(s) => {
            term.write_line("Hello World!")
                .expect("Failed to write to terminal");
            if !debug {
                UI.init_spinner.set_message("Validating...");
                UI.init_spinner
                    .enable_steady_tick(Duration::from_millis(100));
            }
            if !debug {
                UI.init_spinner.finish_and_clear();
            }
            term.write_line(&format!("Validation done {}", s))?;

            if debug {
                term.write_line("Finding photos")?;
            } else {
                UI.indexing_spinner.set_message("Finding photos...");
                UI.indexing_spinner
                    .enable_steady_tick(Duration::from_millis(100));
            }
        }
        ProgressEvent::MediaFilesCalculated(total_files) => {
            if !debug {
                UI.indexing_spinner.finish_and_clear();
            }

            term.write_line(&format!("Total photos {}", total_files))?;
            if !debug {
                UI.media_progress.set_length(total_files as u64);
            }
        }
        ProgressEvent::MediaFileDone(f) => {
            if debug {
                term.write_line(&format!("  {}", f))?;
            } else {
                UI.media_progress.inc(1);
            }
        }
        ProgressEvent::MediaDone() => {
            if !debug {
                UI.media_progress.finish_and_clear();
            }
        }
        ProgressEvent::AlbumsCalculated(i) => {
            term.write_line(&format!("Total albums {}", i))?;
            if !debug {
                UI.albums_progress.set_length(i as u64);
            }
        }
        ProgressEvent::AlbumFileDone() => {
            if !debug {
                UI.albums_progress.inc(1);
            }
        }
        ProgressEvent::AlbumsDone() => {
            if !debug {
                UI.albums_progress.finish_and_clear();
            }
        }
        ProgressEvent::AllDone() => {
            term.write_line("Done")?;
        }
    }
    Ok(())
}

enum ProgressEvent {
    Start(String),
    MediaFilesCalculated(u32),
    MediaFileDone(String),
    MediaDone(),
    AlbumsCalculated(u32),
    AlbumFileDone(),
    AlbumsDone(),
    AllDone(),
}
