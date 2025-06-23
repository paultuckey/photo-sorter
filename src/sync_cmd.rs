use crate::album::detect_album;
use crate::file_type::{QuickFileType, QuickScannedFile, quick_file_scan};
use crate::markdown_cmd::{assemble_markdown, mfm_from_media_file_info};
use crate::media::{MediaFileInfo, media_file_info_from_readable};
use crate::util::{PsContainer, PsZipContainer, PsDirectoryContainer};
use anyhow::anyhow;
use console::Term;
use futures::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressIterator};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::{fs, thread, time::Duration};
use tokio::time;
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
    input: &String,
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
    input: &String,
    output_directory: &Option<String>,
    skip_markdown: bool,
) -> anyhow::Result<()> {
    tx.send(ProgressEvent::Start(input.clone()))?;
    let path = Path::new(input);
    if !path.exists() {
        return Err(anyhow!("Input path does not exist: {}", input));
    }
    let mut container: Box<dyn PsContainer>;
    if path.is_dir() {
        container = Box::new(PsDirectoryContainer::new(input.clone()));
    } else {
        container = Box::new(PsZipContainer::new(input.clone()));
    }

    let files = container.scan();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let quick_scanned_files = quick_file_scan(&container, files);

    let quick_media_files = quick_scanned_files
        .iter()
        .filter(|m| m.quick_file_type == QuickFileType::Media)
        .collect::<Vec<&QuickScannedFile>>();
    tx.send(ProgressEvent::MediaFilesCalculated(
        quick_media_files.len() as u32
    ))?;

    let supplemental_paths = quick_scanned_files
        .iter()
        .filter(|m| m.supplemental_json_file.is_some())
        .collect::<Vec<&QuickScannedFile>>();
    let mut json_hashmap: HashMap<String, Vec<u8>> = HashMap::new();
    debug!("Found {} supplemental files", supplemental_paths.len());
    for qsf in supplemental_paths {
        let Some(path) = qsf.supplemental_json_file.clone() else {
            continue;
        };
        let bytes = container.file_bytes(&path);
        let Ok(bytes) = bytes else {
            debug!("Could not read supplemental json file: {}", path);
            continue;
        };
        debug!("Read supplemental json file: {}", path);
        json_hashmap.insert(path, bytes);
    }

    let num_threads = 1; // should be related to number of cores on underlying machine
    let fetches = stream::iter(
        quick_media_files //
            .iter()
            .map(|quick_scanned_file| {
                let tx2 = tx.clone();
                let bytes = container.file_bytes(&quick_scanned_file.name.clone());
                let json_hashmap2 = &json_hashmap;
                async move {
                    let Ok(bytes) = bytes else {
                        debug!("Could not read file: {}", quick_scanned_file.name);
                        return Err(anyhow!("Could not read file: {}", quick_scanned_file.name));
                    };
                    let scanned_file = analyze(
                        bytes,
                        quick_scanned_file,
                        dry_run,
                        output_directory,
                        skip_markdown,
                        json_hashmap2,
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
    time::sleep(Duration::from_millis(1000)).await;

    let quick_album_files = quick_scanned_files
        .iter()
        .filter(|m| m.quick_file_type == QuickFileType::Album)
        .collect::<Vec<&QuickScannedFile>>();
    tx.send(ProgressEvent::AlbumsCalculated(
        quick_album_files.len() as u32
    ))?;
    time::sleep(Duration::from_millis(1000)).await;

    for quick_album in quick_album_files {
        let album_o = detect_album(&mut container, quick_album);
        let Some(album) = album_o else {
            continue;
        };
        tx.send(ProgressEvent::AlbumFileDone(album.name))?;
    }
    tx.send(ProgressEvent::AlbumsDone())?;
    time::sleep(Duration::from_millis(1000)).await;

    tx.send(ProgressEvent::AllDone())?;
    time::sleep(Duration::from_millis(1000)).await;
    Ok(())
}

pub(crate) fn analyze(
    bytes: Vec<u8>,
    qsf: &QuickScannedFile,
    dry_run: bool,
    output_directory: &Option<String>,
    skip_markdown: bool,
    extra_files: &HashMap<String, Vec<u8>>,
) -> anyhow::Result<MediaFileInfo> {
    let file = &qsf.name;
    debug!("Analyzing {:?}", file);
    let extra_info_path = qsf.supplemental_json_file.clone();
    let mut extra_info_bytes: Option<Vec<u8>> = None;
    if let Some(path) = extra_info_path.clone() {
        if let Some(b) = extra_files.get(&path) {
            extra_info_bytes = Some(b.clone());
        } else {
            debug!("No extra info file found for: {:?}", path);
        }
    }
    //let f_box = container.file_bytes(&file.clone());
    //let f = f_box.as_ref();

    let media_file_info_res = media_file_info_from_readable(&bytes, &qsf.name, &extra_info_bytes);
    let Ok(media_file) = media_file_info_res else {
        debug!("File type unsupported: {:?}", file);
        return Err(anyhow!("Unsupported file type: {:?}", file));
    };

    if let Some(output_dir) = output_directory {
        if let Some(d) = media_file.desired_media_path.clone() {
            let output_path = Path::new(output_dir).join(&d);
            if output_path.exists() {
                debug!(
                    "Output path exists, need to check checksum {:?}",
                    output_path
                );
                // todo: check if output_path exists, if so, use checksum
            }
            if dry_run {
                debug!("Would create media file at {:?}", output_path);
            } else {
                // todo fs::create_dir_all(output_path.parent().unwrap())?;
                fs::write(&output_path, bytes)?;
                debug!("Created file at {:?}", output_path);
            }
        }
        if !skip_markdown {
            if let Some(m) = media_file.desired_markdown_path.clone() {
                let output_path = Path::new(output_dir).join(&m);
                if output_path.exists() {
                    debug!("Markdown file already exists at {:?}", output_path);
                    // todo: grab existing content and discard frontmatter
                }
                let mfm = mfm_from_media_file_info(&media_file);
                let s = assemble_markdown(&mfm, &"".to_string())?;
                if dry_run {
                    debug!("Would create markdown file at {:?}", output_path);
                } else {
                    fs::create_dir_all(output_path.parent().unwrap())?;
                    fs::write(&output_path, s)?;
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
            term.write_line(&format!("Input {}", s))
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
        ProgressEvent::MediaFileDone(_) => {
            if !debug {
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
        ProgressEvent::AlbumFileDone(_) => {
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
    AlbumFileDone(String),
    AlbumsDone(),
    AllDone(),
}
