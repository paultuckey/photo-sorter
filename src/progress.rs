use status_line::StatusLine;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Progress {
    total: u64,
    current: Arc<AtomicU64>,
}
impl Progress {
    pub(crate) fn new(total: u64) -> StatusLine<Progress> {
        StatusLine::new(Progress {
            current: Arc::new(AtomicU64::new(0)),
            total,
        })
    }
    pub(crate) fn inc(&self) {
        self.current.fetch_add(1, Ordering::Relaxed);
    }
}

impl Display for Progress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let current = self.current.load(Ordering::Relaxed);
        let progress_bar_char_width = 19; // plus on for arrow head

        let pos = if self.total == 0 {
            0
        } else {
            (progress_bar_char_width * current / self.total).min(progress_bar_char_width)
        };

        let bar_done = "=".repeat(pos as usize);
        let bar_not_done = " ".repeat(progress_bar_char_width as usize - pos as usize);
        let x_of_y = format!("{} of {}", current, self.total);
        write!(f, "[{bar_done}>{bar_not_done}] {x_of_y}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tracing::debug;

    /// Progress example (not really a test)
    /// increase delay to make it more visible as progress bar has a frame rate
    #[test]
    fn test_progress() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let delay = Duration::from_millis(100);
        let prog = Progress::new(10);
        thread::sleep(delay);
        for i in 0..10 {
            prog.inc();
            if i % 2 == 0 {
                debug!("Even {i}");
            }
            thread::sleep(delay);
        }
        Ok(())
    }

    #[test]
    fn test_display_basic() {
        // 0%
        let p = Progress {
            current: Arc::new(AtomicU64::new(0)),
            total: 100,
        };
        assert_eq!(
            format!("{}", p),
            "[>                   ] 0 of 100"
        );

        // 50%
        let p = Progress {
            current: Arc::new(AtomicU64::new(50)),
            total: 100,
        };
        // 19 * 50 / 100 = 9.5 -> 9
        // [========= >          ] ?
        // pos=9.
        // done: 9 "="
        // not done: 10 " "
        assert_eq!(
            format!("{}", p),
            "[=========>          ] 50 of 100"
        );

        // 100%
        let p = Progress {
            current: Arc::new(AtomicU64::new(100)),
            total: 100,
        };
        // pos=19
        // done: 19 "="
        // not done: 0 " "
        assert_eq!(
            format!("{}", p),
            "[===================>] 100 of 100"
        );
    }

    #[test]
    fn test_display_zero_total() {
        let p = Progress {
            current: Arc::new(AtomicU64::new(0)),
            total: 0,
        };
        // Should handle total=0 gracefully (0% progress)
        assert_eq!(
            format!("{}", p),
            "[>                   ] 0 of 0"
        );
    }

    #[test]
    fn test_display_overflow() {
        let p = Progress {
            current: Arc::new(AtomicU64::new(150)),
            total: 100,
        };
        // Should cap at 100% visual progress
        assert_eq!(
            format!("{}", p),
            "[===================>] 150 of 100"
        );
    }
}
