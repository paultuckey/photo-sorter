use crate::album::{build_album_md, parse_album};
use crate::dedup::{DeDuplicationResult, Deduplicator};
use crate::file_type::QuickFileType;
use crate::fs::{FileSystem, OsFileSystem, ZipFileSystem};
use crate::inspect::inspect_media_files;
use crate::markdown::sync_markdown;
use crate::media::{MediaFileDerivedInfo, MediaFileInfo, media_file_derived_from_media_info};
use crate::progress::Progress;
use crate::util::{ScanInfo, scan_fs};
use anyhow::anyhow;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub(crate) fn main(
    dry_run: bool,
    input: &String,
    output_directory: &Option<String>,
    skip_markdown: bool,
    skip_media: bool,
    skip_albums: bool,
) -> anyhow::Result<()> {
    let path = Path::new(input);
    if !path.exists() {
        return Err(anyhow!("Input path does not exist: {}", input));
    }
    let container: Arc<dyn FileSystem> = if path.is_dir() {
        info!("Input directory: {input}");
        Arc::new(OsFileSystem::new(input))
    } else {
        info!("Input zip: {input}");
        let tz = chrono::Local::now().offset().to_owned();
        Arc::new(ZipFileSystem::new(input, tz)?)
    };

    let files = scan_fs(container.as_ref());
    info!("Found {} files in input", files.len());

    let mut output_container_o: Option<OsFileSystem> = None;
    if let Some(output) = output_directory {
        info!("Output directory: {output}");
        let output_container = OsFileSystem::new(output);
        if !output_container.root_exists() {
            warn!("Output directory does not exist {output}");
        }
        output_container_o = Some(output_container);
    }
    let mut deduper = Deduplicator::new();
    let mut final_path_by_original_path = HashMap::<String, String>::new();

    if !skip_media {
        let media_si_files: Vec<ScanInfo> = files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::Media)
            .cloned()
            .collect();
        info!("Inspecting {} photo and video files", media_si_files.len());
        let prog = Arc::new(Progress::new(media_si_files.len() as u64));
        // Inspection (hashing + metadata) runs in parallel; dedup must stay on
        // this thread since it mutates the shared collection. Files with the
        // same content hash collapse into one entry, recording each original
        // path (see `Deduplicator`).
        for media in inspect_media_files(container.clone(), media_si_files, prog.clone()) {
            deduper.add(media);
        }
        drop(prog);

        if let Some(ref mut output_container) = output_container_o {
            let media_to_write = deduper.sorted_media();
            info!("Outputting {} photo and video files", media_to_write.len());
            let prog = Progress::new(media_to_write.len() as u64);
            for media in media_to_write {
                prog.inc();
                let derived = media_file_derived_from_media_info(media)?;
                let write_r = write_media(
                    media,
                    &derived,
                    dry_run,
                    container.as_ref(),
                    output_container,
                );
                match write_r {
                    Ok(final_path) => {
                        let long_checksum = &media.hash_info.long_checksum;
                        final_path_by_original_path
                            .insert(long_checksum.clone(), final_path.clone());
                        if !skip_markdown {
                            let sync_md_r =
                                sync_markdown(dry_run, media, &derived, output_container);
                            if let Err(e) = sync_md_r {
                                warn!(
                                    "Error writing markdown file: {:?}, error: {}",
                                    derived.desired_media_path, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Error writing media file: {:?}, error: {}",
                            derived.desired_media_path, e
                        );
                    }
                }
            }
            drop(prog);
        }
    }

    if !skip_albums {
        let scan_info_albums = files
            .iter()
            .filter(|m| {
                m.quick_file_type == QuickFileType::AlbumCsv
                    || m.quick_file_type == QuickFileType::AlbumJson
            })
            .collect::<Vec<&ScanInfo>>();
        info!("Inspecting {} album files", scan_info_albums.len());
        let mut albums = vec![];
        let prog = Progress::new(deduper.count() as u64);
        for si in scan_info_albums {
            prog.inc();
            let album_o = parse_album(container.as_ref(), si, &files);
            let Some(album) = album_o else {
                continue;
            };
            albums.push(album);
        }
        drop(prog);

        if let Some(ref output_container) = output_container_o {
            info!("Outputting {} albums", albums.len());
            for album in albums {
                let a_s = build_album_md(
                    &album,
                    Some(deduper.by_checksum()),
                    "../",
                    Some(&final_path_by_original_path),
                );
                let output_path = &album.desired_album_md_path;
                if output_container.exists(&album.desired_album_md_path) {
                    debug!("  Album markdown file already exists, clobbering, at {output_path:?}");
                }
                let bytes = a_s.as_bytes().to_vec();
                output_container.write(dry_run, output_path, Cursor::new(bytes));
            }
        }
    }

    Ok(())
}

pub(crate) fn write_media(
    media_file: &MediaFileInfo,
    derived: &MediaFileDerivedInfo,
    dry_run: bool,
    input_container: &dyn FileSystem,
    output_container: &OsFileSystem,
) -> anyhow::Result<String> {
    info!("Output {:?}", derived.desired_media_path);

    let desired_output_path_with_ext =
        match Deduplicator::resolve_output_path(media_file, derived, output_container)? {
            DeDuplicationResult::SkipWrite(path) => return Ok(path),
            DeDuplicationResult::WritePath(path) => path,
        };
    let reader = input_container.open(&media_file.original_file_this_run)?;
    output_container.write(dry_run, &desired_output_path_with_ext.clone(), reader);
    output_container.set_modified(
        dry_run,
        &desired_output_path_with_ext.clone(),
        &media_file.modified,
    );
    Ok(desired_output_path_with_ext)
}
