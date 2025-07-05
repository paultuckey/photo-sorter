use crate::album::{build_album_md, parse_csv_album, parse_json_album};
use crate::file_type::{QuickFileType, QuickScannedFile, quick_scan_files};
use crate::markdown_cmd::{sync_markdown};
use crate::media::{media_file_info_from_readable, MediaFileInfo};
use crate::util::{PsContainer, PsDirectoryContainer, PsZipContainer, is_existing_file_same, checksum_bytes, Progress};
use anyhow::anyhow;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

pub(crate) async fn main(
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
    let mut container: Box<dyn PsContainer>;
    if path.is_dir() {
        container = Box::new(PsDirectoryContainer::new(input.clone()));
    } else {
        container = Box::new(PsZipContainer::new(input.clone()));
    }
    info!("Input zip: {}", input);


    info!("Indexing...");
    let files = container.scan();
    let quick_scanned_files = quick_scan_files(&container, &files);
    info!("Indexed {} files in zip", files.len());

    let mut output_container_o: Option<PsDirectoryContainer> = None;
    if let Some(output) = output_directory {
        output_container_o = Some(PsDirectoryContainer::new(output.clone()));
    }
    let mut all_media = HashMap::<String, MediaFileInfo>::new();

    if !skip_media {
        let supplemental_paths = quick_scanned_files
            .iter()
            .filter(|m| m.supplemental_json_file.is_some())
            .collect::<Vec<&QuickScannedFile>>();
        let mut json_hashmap: HashMap<String, Vec<u8>> = HashMap::new();
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
        info!("Read {} supplemental files", json_hashmap.len());

        let quick_media_files = quick_scanned_files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::Media)
            .collect::<Vec<&QuickScannedFile>>();
        info!("Inspecting {} photo and video files", quick_media_files.len());
        let prog = Progress::new(quick_media_files.len() as u64);
        for quick_scanned_file in quick_media_files {
            prog.inc();
            let bytes = container.file_bytes(&quick_scanned_file.name.clone());
            let Ok(bytes) = bytes else {
                debug!("Could not read file: {}", quick_scanned_file.name);
                return Err(anyhow!("Could not read file: {}", quick_scanned_file.name));
            };
            let _ = read_media(bytes, quick_scanned_file, &mut all_media, &json_hashmap);
        }
        drop(prog);

        if let Some(ref mut output_container) = output_container_o {
            let prog = Progress::new(all_media.len() as u64);
            for media in all_media.values() {
                prog.inc();
                let _ = write_media(media, dry_run, &mut container, output_container);
                if !skip_markdown {
                    let _ = sync_markdown(dry_run, media, output_container);
                }
            }
            drop(prog);
        }
    }

    if !skip_albums {
        let quick_scanned_albums = quick_scanned_files
            .iter()
            .filter(|m|
                m.quick_file_type == QuickFileType::AlbumCsv
                    || m.quick_file_type == QuickFileType::AlbumJson)
            .collect::<Vec<&QuickScannedFile>>();
        info!("Inspecting {} albums", quick_scanned_albums.len());
        let mut albums = vec![];
        let prog = Progress::new(all_media.len() as u64);
        for qsf in quick_scanned_albums {
            prog.inc();
            match qsf.quick_file_type {
                QuickFileType::AlbumCsv => {
                    let album_o = parse_csv_album(&mut container, qsf);
                    let Some(album) = album_o else {
                        continue;
                    };
                    albums.push(album);
                }
                QuickFileType::AlbumJson => {
                    let album_o = parse_json_album(&mut container, qsf, &all_media);
                    let Some(album) = album_o else {
                        continue;
                    };
                    albums.push(album);
                }
                _ => {}
            }
        }
        drop(prog);

        info!("Generating {} albums", albums.len());
        for album in albums {
            let a_s = build_album_md(&album);
            let Some(output_c) = &output_container_o else {
                continue;
            };
            let output_path = &album.desired_album_md_path;
            if output_c.exists(&album.desired_album_md_path) {
                debug!("Album markdown file already exists, clobbering, at {:?}", output_path);
            }
            let bytes = &a_s.as_bytes().to_vec();
            output_c.write(dry_run, output_path, bytes);
        }
        info!("Done albums");
    }

    Ok(())
}

/// Take a media file and:
/// - generate a checksum
/// - check if it already exists in the media map
/// - capture extra_info
/// - populate exif data
pub(crate) fn read_media(
    bytes: Vec<u8>,
    qsf: &QuickScannedFile,
    all_media: &mut HashMap<String, MediaFileInfo>,
    extra_files: &HashMap<String, Vec<u8>>,
) -> anyhow::Result<()> {
    let checksum_o = checksum_bytes(&bytes).ok();
    let Some((short_checksum, long_checksum)) = checksum_o else {
        debug!("Could not calculate checksum for file: {:?}", qsf.name);
        return Err(anyhow!("Could not calculate checksum for file: {:?}", qsf.name));
    };
    debug!("File {:?} has checksum: {}", qsf.name, short_checksum);
    if let Some(m) = all_media.get_mut(&long_checksum) {
        m.original_path.push(qsf.name.clone());
        return Ok(());
    }
    let extra_info_path = qsf.supplemental_json_file.clone();
    let mut extra_info_bytes: Option<Vec<u8>> = None;
    if let Some(path) = extra_info_path.clone() {
        if let Some(b) = extra_files.get(&path) {
            extra_info_bytes = Some(b.clone());
        } else {
            debug!("No extra info file found for: {:?}", path);
        }
    }
    let media_file_info_res = media_file_info_from_readable(
        qsf, &bytes, &extra_info_bytes, &short_checksum, &long_checksum);
    let Ok(media_file) = media_file_info_res else {
        debug!("File type unsupported: {:?}", qsf.name);
        return Err(anyhow!("File type unsupported: {:?}", qsf.name));
    };
    all_media.insert(media_file.long_checksum.clone(), media_file.clone());
    Ok(())
}

pub(crate) fn write_media(
    media_file: &MediaFileInfo,
    dry_run: bool,
    input_container: &mut Box<dyn PsContainer>,
    output_container: &mut PsDirectoryContainer,
) -> anyhow::Result<()> {
    let Some(desired_output_path) = &media_file.desired_media_path else {
        debug!("No desired media path for file: {:?}", media_file);
        return Err(anyhow!("No desired media path for file: {:?}", media_file));
    };
    if output_container.exists(desired_output_path) {
        let es_o =
            is_existing_file_same(output_container, &media_file.long_checksum, desired_output_path);
        if let Some(existing_same) = es_o {
            if !existing_same {
                warn!("File with different checksum is at {:?}", desired_output_path);
                return Err(anyhow!("File clash: {:?}", desired_output_path));
            }
            debug!("No need to write, file already exists with same checksum at {:?}", desired_output_path);
        }
    } else {
        let bytes = input_container.file_bytes(desired_output_path)?;
        output_container.write(dry_run, &desired_output_path.clone(), &bytes);
    }
    Ok(())
}
