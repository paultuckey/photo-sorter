use console::Term;
use indicatif::ProgressBar;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::{thread, time::Duration};


pub(crate) fn main() -> anyhow::Result<()> {

    let (tx, rx) = mpsc::channel();

    let t1 = thread::spawn(move || {
        send_events(tx).unwrap();
    });

    let t2 = thread::spawn(move || {
        let term = Term::stdout();
        let bar1 = ProgressBar::new_spinner();
        let bar2 = ProgressBar::new(1);
        let bar3 = ProgressBar::new(1);

        loop {
            let event = rx.recv();
            match event {
                Ok(e) => {
                    handle(&term, &bar1, &bar2, &bar3, e).unwrap();
                }
                Err(_) => {
                    break; // Exit the loop if the channel is closed
                }
            }
        }
    });

    t1.join().unwrap();
    t2.join().unwrap();
    // let bar1 = ProgressBar::new_spinner();
    // bar1.set_message("Loading...");
    // bar1.enable_steady_tick(Duration::from_millis(100));
    // thread::sleep(Duration::from_millis(5000));
    // bar1.finish_and_clear();
    // term.write_line("Loading  done")?;

    // term.write_line("bar2")?;
    // let bar2 = ProgressBar::new(100);
    // for _ in 0..100 {
    //     bar2.inc(1);
    //     thread::sleep(Duration::from_millis(20));
    // }
    // bar2.finish();

    Ok(())
}

/// steps:
///  - check inputs and files exits
///  - index zip counting media+json, albums
///    - one progress bar
///  - output media+markdown
///  - output albums
fn send_events(tx: Sender<ProgressEvent>) -> anyhow::Result<()> {
    tx.send(ProgressEvent::Start())?;

    thread::sleep(Duration::from_millis(1000));
    tx.send(ProgressEvent::InputVerified("hello".to_string()))?;

    thread::sleep(Duration::from_millis(1000));
    tx.send(ProgressEvent::TotalFilesCalculated(100, 100))?;

    thread::sleep(Duration::from_millis(1000));
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(10));
        tx.send(ProgressEvent::MediaFileDone())?;
    }
    thread::sleep(Duration::from_millis(1000));
    tx.send(ProgressEvent::MediaDone())?;

    thread::sleep(Duration::from_millis(1000));
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(10));
        tx.send(ProgressEvent::AlbumFileDone())?;
    }
    thread::sleep(Duration::from_millis(2000));
    tx.send(ProgressEvent::AlbumsDone())?;

    thread::sleep(Duration::from_millis(2000));
    tx.send(ProgressEvent::AllDone())?;
    Ok(())
}

fn handle(
    term: &Term,
    loading_spinner: &ProgressBar,
    media_progress: &ProgressBar,
    albums_progress: &ProgressBar,
    e: ProgressEvent,
) -> anyhow::Result<()> {
    match e {
        ProgressEvent::Start() => {
            term.write_line("Hello World!")
                .expect("Failed to write to terminal");
        }
        ProgressEvent::InputVerified(f) => {
            term.write_line(&format!("Loading done {}", f))?;

            loading_spinner.set_message("Loading...");
            loading_spinner.enable_steady_tick(Duration::from_millis(100));
        }
        ProgressEvent::TotalFilesCalculated(total_files, total_albums) => {
            loading_spinner.finish_and_clear();

            term.write_line(&format!(
                "Total files {}, albums {}",
                total_files, total_albums
            ))?;
            media_progress.set_length(total_files as u64);
        }
        ProgressEvent::MediaFileDone() => {
            media_progress.inc(1);
        }
        ProgressEvent::MediaDone() => {
            media_progress.finish_and_clear();
        }
        ProgressEvent::AlbumFileDone() => {
            albums_progress.inc(1);
        }
        ProgressEvent::AlbumsDone() => {
            albums_progress.finish_and_clear();
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
    TotalFilesCalculated(u32, u32),
    MediaFileDone(),
    MediaDone(),
    AlbumFileDone(),
    AlbumsDone(),
    AllDone(),
}
