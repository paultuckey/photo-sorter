use crate::takeout_reader;
use console::Term;
use indicatif::ProgressBar;
use once_cell::sync::Lazy;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::{thread, time::Duration};

struct State {
    init_spinner: ProgressBar,
    indexing_spinner: ProgressBar,
    media_progress: ProgressBar,
    albums_progress: ProgressBar,
}

static UI: Lazy<State> = Lazy::new(|| {
    return State {
        init_spinner: ProgressBar::new_spinner(),
        indexing_spinner: ProgressBar::new_spinner(),
        media_progress: ProgressBar::new(32),
        albums_progress: ProgressBar::new(32),
    };
});

pub(crate) fn main(
    directory: &Option<String>,
    input_takeout: &Option<String>,
    input_icloud: &Option<String>,
    dry_run: &bool,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel();

    let terminal_output_thread = thread::spawn(move || {
        let term = Term::stdout();
        while let Ok(event) = rx.recv() {
            handle(&term, event).unwrap();
        }
    });

    send_events(tx, &input_takeout)?;
    terminal_output_thread.join().unwrap();

    Ok(())
}

fn send_events(tx: Sender<ProgressEvent>, input_takeout: &Option<String>) -> anyhow::Result<()> {
    tx.send(ProgressEvent::Start())?;
    thread::sleep(Duration::from_millis(1000));

    tx.send(ProgressEvent::InputVerified(input_takeout.clone().unwrap()))?;
    thread::sleep(Duration::from_millis(1000));

    thread::sleep(Duration::from_millis(1000));
    let c = takeout_reader::count(&input_takeout.clone().unwrap())?;
    let total_albums = 5;
    tx.send(ProgressEvent::MediaFilesCalculated(c))?;
    thread::sleep(Duration::from_millis(1000));

    for _ in 0..c {
        tx.send(ProgressEvent::MediaFileDone())?;
        thread::sleep(Duration::from_millis(10));
    }
    tx.send(ProgressEvent::MediaDone())?;
    thread::sleep(Duration::from_millis(1000));

    tx.send(ProgressEvent::AlbumsCalculated(total_albums))?;
    thread::sleep(Duration::from_millis(1000));

    for _ in 0..total_albums {
        tx.send(ProgressEvent::AlbumFileDone())?;
        thread::sleep(Duration::from_millis(1));
    }
    tx.send(ProgressEvent::AlbumsDone())?;
    thread::sleep(Duration::from_millis(1000));

    tx.send(ProgressEvent::AllDone())?;
    thread::sleep(Duration::from_millis(1000));
    Ok(())
}

fn handle(term: &Term, e: ProgressEvent) -> anyhow::Result<()> {
    match e {
        ProgressEvent::Start() => {
            term.write_line("Hello World!")
                .expect("Failed to write to terminal");
            UI.init_spinner.set_message("Validating...");
            UI.init_spinner
                .enable_steady_tick(Duration::from_millis(100));
        }
        ProgressEvent::InputVerified(f) => {
            UI.init_spinner.finish_and_clear();
            term.write_line(&format!("Validation done {}", f))?;

            UI.indexing_spinner.set_message("Finding photos...");
            UI.indexing_spinner
                .enable_steady_tick(Duration::from_millis(100));
        }
        ProgressEvent::MediaFilesCalculated(total_files) => {
            UI.indexing_spinner.finish_and_clear();

            term.write_line(&format!("Total photos {}", total_files))?;
            UI.media_progress.set_length(total_files as u64);
        }
        ProgressEvent::MediaFileDone() => {
            UI.media_progress.inc(1);
        }
        ProgressEvent::MediaDone() => {
            UI.media_progress.finish_and_clear();
        }
        ProgressEvent::AlbumsCalculated(i) => {
            term.write_line(&format!("Total albums {}", i))?;
            UI.albums_progress.set_length(i as u64);
        }
        ProgressEvent::AlbumFileDone() => {
            UI.albums_progress.inc(1);
        }
        ProgressEvent::AlbumsDone() => {
            UI.albums_progress.finish_and_clear();
        }
        ProgressEvent::AllDone() => {
            term.write_line("Done")?;
        }
    }
    Ok(())
}

enum ProgressEvent {
    Start(),
    InputVerified(String),
    MediaFilesCalculated(u32),
    MediaFileDone(),
    MediaDone(),
    AlbumsCalculated(u32),
    AlbumFileDone(),
    AlbumsDone(),
    AllDone(),
}
