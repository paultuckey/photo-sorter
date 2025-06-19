use crate::takeout_reader;
use console::Term;
use indicatif::ProgressBar;
use once_cell::sync::Lazy;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::{thread, time::Duration};
use futures::StreamExt;
use crate::takeout_reader::PsFileInZip;

struct State {
    init_spinner: ProgressBar,
    indexing_spinner: ProgressBar,
    media_progress: ProgressBar,
    albums_progress: ProgressBar,
}

static UI: Lazy<State> = Lazy::new(|| {
    State {
        init_spinner: ProgressBar::new_spinner(),
        indexing_spinner: ProgressBar::new_spinner(),
        media_progress: ProgressBar::new(32),
        albums_progress: ProgressBar::new(32),
    }
});

pub(crate) async fn main(
    directory: &Option<String>,
    input_takeout: &Option<String>,
    input_icloud: &Option<String>,
    debug: &bool,
    dry_run: &bool,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel();

    let debug2 = debug.clone();
    let terminal_output_thread = thread::spawn(move || {
        let term = Term::stdout();
        while let Ok(event) = rx.recv() {
            handle(&term, event, debug2).unwrap();
        }
    });

    start(tx, input_takeout, dry_run).await?;
    terminal_output_thread.join().unwrap();

    Ok(())
}

async fn start(tx: Sender<ProgressEvent>, input_takeout: &Option<String>, dry_run: &bool) -> anyhow::Result<()> {
    let s = input_takeout.clone().unwrap();
    tx.send(ProgressEvent::Start(s.clone()))?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let files = takeout_reader::scan(&s)?;
    tx.send(ProgressEvent::MediaFilesCalculated(files.len() as u32))?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // scan zip and collect list of files with right extensions and file size
    // then find any extra files for meta info
    // find albums (based on paths)
    // and then analyze each file
    
    
    
    let num_threads = 10; // should be related to number of cored on underlying machine
    let fetches = futures::stream::iter(
        files.iter().map(|file| {
            let s2 = s.clone();
            let tx2 = tx.clone();
            async move {
                let scanned_file = takeout_reader::analyze(&file, &s2, &dry_run);
                tx2.send(ProgressEvent::MediaFileDone(file.clone())).expect("send media file");
                scanned_file
            }
        })
    ).buffer_unordered(num_threads).collect::<Vec<anyhow::Result<PsFileInZip>>>();
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
