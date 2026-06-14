use crate::fs::FileSystem;
use crate::media::{MediaFileInfo, media_file_info_from_readable};
use crate::progress::Progress;
use crate::supplemental_info::{
    PsSupplementalInfo, detect_supplemental_info, load_supplemental_info,
};
use crate::util::{ScanInfo, checksum_bytes};
use anyhow::anyhow;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use tracing::{debug, info, warn};

/// Hash and parse media files in parallel, yielding a [`MediaFileInfo`] as each
/// one finishes.
///
/// A pool of rayon workers inspect the files concurrently and push results
/// through a bounded channel; the returned iterator drains that channel on the
/// calling thread. Streaming results (rather than collecting them) lets the
/// caller fold each item straight into sqlite or a dedup map without holding the
/// whole library in memory.
///
/// `container` and `prog` are taken as [`Arc`]s because the worker thread
/// outlives this call (it is owned by the returned iterator), so they can't be
/// borrowed from the caller's stack.
pub(crate) fn inspect_media_files(
    container: Arc<dyn FileSystem>,
    media_si_files: Vec<ScanInfo>,
    prog: Arc<Progress>,
) -> impl Iterator<Item = MediaFileInfo> {
    // Bound the channel so fast parallel producers can't outrun the single
    // consumer and pile up in memory.
    let channel_capacity = rayon::current_num_threads().saturating_mul(4).max(1);
    let (tx, rx) = std::sync::mpsc::sync_channel(channel_capacity);

    let handle = std::thread::spawn(move || {
        media_si_files.par_iter().for_each(|media_si| {
            if let Ok(Some(info)) = analyze_file(container.as_ref(), media_si) {
                let _ = tx.send(info);
            }
            prog.inc();
        });
    });

    InspectMediaIter {
        rx,
        handle: Some(handle),
    }
}

/// Iterator over inspected media that owns the producer thread, joining it once
/// the channel drains (or on drop) so the worker never outlives the iterator.
struct InspectMediaIter {
    rx: Receiver<MediaFileInfo>,
    handle: Option<JoinHandle<()>>,
}

impl Iterator for InspectMediaIter {
    type Item = MediaFileInfo;

    fn next(&mut self) -> Option<Self::Item> {
        if let Ok(info) = self.rx.recv() {
            return Some(info);
        }
        // Channel closed: the producer dropped its sender, so it is done. Join
        // to reclaim the thread and re-raise any worker panic, matching the
        // previous scoped-thread behavior where a panic aborted the run.
        if let Some(handle) = self.handle.take()
            && let Err(panic) = handle.join()
        {
            std::panic::resume_unwind(panic);
        }
        None
    }
}

impl Drop for InspectMediaIter {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        // The consumer stopped early. Drain so a producer parked on the full
        // bounded channel can finish and drop its sender, then join rather than
        // leaving the worker detached. Don't re-raise a panic here: a drop may
        // run while already unwinding, and a double panic aborts the process.
        for _ in self.rx.iter() {}
        let _ = handle.join();
    }
}

/// Inspect a single media file: load any supplemental info, checksum the bytes,
/// then derive its type and metadata. Returns `Ok(None)` when the file isn't a
/// supported media type, and `Err` when it can't be read or hashed.
fn analyze_file(
    root: &dyn FileSystem,
    media_si: &ScanInfo,
) -> anyhow::Result<Option<MediaFileInfo>> {
    let mut supp_info_o = None;
    let supp_info_path_o = detect_supplemental_info(&media_si.file_path.clone(), root);
    if let Some(supp_info_path) = supp_info_path_o {
        supp_info_o = load_supplemental_info(&supp_info_path, root);
    }

    let reader = root.open(&media_si.file_path.clone())?;
    let hash_info_o = checksum_bytes(reader).ok();
    let Some(hash_info) = hash_info_o else {
        debug!(
            "Could not calculate checksum for file: {:?}",
            media_si.file_path
        );
        return Err(anyhow!(
            "Could not calculate checksum for file: {:?}",
            media_si.file_path
        ));
    };

    let media_info_r = media_file_info_from_readable(media_si, root, &supp_info_o, &hash_info);
    match media_info_r {
        Ok(media_info) => Ok(Some(media_info)),
        Err(_) => Ok(None),
    }
}

/// Take a media file and:
/// - generate a checksum
/// - check if it already exists in the media map
/// - capture extra_info
/// - populate exif data
pub(crate) fn inspect_media(
    si: &ScanInfo,
    root: &dyn FileSystem,
    all_media: &mut HashMap<String, MediaFileInfo>,
    supp_info: &Option<PsSupplementalInfo>,
) -> anyhow::Result<MediaFileInfo> {
    info!("Inspect: {}", si.file_path);
    let reader = root.open(&si.file_path.to_string())?;
    let hash_info_o = checksum_bytes(reader).ok();
    let Some(hash_info) = hash_info_o else {
        warn!("Could not calculate checksum for: {:?}", si.file_path);
        return Err(anyhow!(
            "Could not calculate checksum for file: {:?}",
            si.file_path
        ));
    };

    debug!("  Checksum calculated: {}", hash_info.long_checksum);
    if let Some(m) = all_media.get_mut(&hash_info.long_checksum) {
        m.original_path.push(si.file_path.clone());
        return Ok(m.clone());
    }
    let media_file_info_res = media_file_info_from_readable(si, root, supp_info, &hash_info);
    let Ok(media_file) = media_file_info_res else {
        warn!("Could not calculate info for: {:?}", si.file_path);
        return Err(anyhow!("File type unsupported: {:?}", si.file_path));
    };
    all_media.insert(hash_info.long_checksum.clone(), media_file.clone());
    Ok(media_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_type::QuickFileType;
    use crate::fs::OsFileSystem;
    use crate::util::scan_fs;

    #[test]
    fn test_inspect_media_files_yields_media() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let container: Arc<dyn FileSystem> = Arc::new(OsFileSystem::new("test"));
        let media_si_files: Vec<ScanInfo> = scan_fs(container.as_ref())
            .into_iter()
            .filter(|m| m.quick_file_type == QuickFileType::Media)
            .collect();
        let prog = Arc::new(Progress::new(media_si_files.len() as u64));

        let results: Vec<MediaFileInfo> =
            inspect_media_files(container, media_si_files, prog).collect();

        assert!(
            results
                .iter()
                .any(|m| m.original_file_this_run == "Canon_40D.jpg")
        );
        assert!(
            results
                .iter()
                .any(|m| m.original_file_this_run == "Hello.mp4")
        );
        Ok(())
    }
}
