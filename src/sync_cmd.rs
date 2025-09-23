use crate::album::{build_album_md, parse_album};
use crate::file_type::QuickFileType;
use crate::markdown::sync_markdown;
use crate::media::{MediaFileInfo, media_file_info_from_readable};
use crate::supplemental_info::{
    SupplementalInfo, detect_supplemental_info, load_supplemental_info,
};
use crate::sync_cmd::DeDuplicationResult::{SkipWrite, WritePath};
use crate::util::{
    Progress, PsContainer, PsDirectoryContainer, PsZipContainer, ScanInfo, checksum_bytes,
    is_existing_file_same,
};
use anyhow::anyhow;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::Path;

const MAX_DUPLICATE_CHECK_ATTEMPTS: i32 = 5;

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
    let mut container: Box<dyn PsContainer>;
    if path.is_dir() {
        info!("Input directory: {input}");
        container = Box::new(PsDirectoryContainer::new(input));
    } else {
        info!("Input zip: {input}");
        let tz = chrono::Local::now().offset().to_owned();
        container = Box::new(PsZipContainer::new(input, tz));
    }

    let files = container.scan();
    info!("Found {} files in input", files.len());

    let mut output_container_o: Option<PsDirectoryContainer> = None;
    if let Some(output) = output_directory {
        info!("Output directory: {output}");
        let output_container = PsDirectoryContainer::new(output);
        if !output_container.root_exists() {
            warn!("Output directory does not exist {output}");
        }
        output_container_o = Some(output_container);
    }
    let mut all_media = HashMap::<String, MediaFileInfo>::new();

    if !skip_media {
        let media_si_files = files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::Media)
            .collect::<Vec<&ScanInfo>>();
        info!("Inspecting {} photo and video files", media_si_files.len());
        let prog = Progress::new(media_si_files.len() as u64);
        for media_si in media_si_files {
            prog.inc();

            let mut supp_info_o = None;
            let supp_info_path_o =
                detect_supplemental_info(&media_si.file_path.clone(), container.as_ref());
            if let Some(supp_info_path) = supp_info_path_o {
                supp_info_o = load_supplemental_info(&supp_info_path, &mut container);
            }
            let bytes = container.file_bytes(&media_si.file_path.clone());
            let Ok(bytes) = bytes else {
                warn!("Could not read file: {}", media_si.file_path);
                return Err(anyhow!("Could not read file: {}", media_si.file_path));
            };
            let _ = inspect_media(&bytes, media_si, &mut all_media, &supp_info_o);
        }
        drop(prog);

        if let Some(ref mut output_container) = output_container_o {
            info!("Outputting {} photo and video files", all_media.len());
            let prog = Progress::new(all_media.len() as u64);
            for media in all_media.values() {
                prog.inc();
                let write_r = write_media(media, dry_run, &mut container, output_container);
                match write_r {
                    Ok(_) => {
                        if !skip_markdown {
                            let _ = sync_markdown(dry_run, media, output_container);
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Error writing media file: {:?}, error: {}",
                            media.desired_media_path, e
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
        let prog = Progress::new(all_media.len() as u64);
        for si in scan_info_albums {
            prog.inc();
            let album_o = parse_album(&mut container, si, &files);
            let Some(album) = album_o else {
                continue;
            };
            albums.push(album);
        }
        drop(prog);

        info!("Outputting {} albums", albums.len());
        for album in albums {
            let a_s = build_album_md(&album, Some(&all_media), "../");
            let Some(output_c) = &output_container_o else {
                continue;
            };
            let output_path = &album.desired_album_md_path;
            if output_c.exists(&album.desired_album_md_path) {
                debug!("  Album markdown file already exists, clobbering, at {output_path:?}");
            }
            let bytes = &a_s.as_bytes().to_vec();
            output_c.write(dry_run, output_path, bytes);
        }
    }

    Ok(())
}

/// Take a media file and:
/// - generate a checksum
/// - check if it already exists in the media map
/// - capture extra_info
/// - populate exif data
pub(crate) fn inspect_media(
    bytes: &Vec<u8>,
    qsf: &ScanInfo,
    all_media: &mut HashMap<String, MediaFileInfo>,
    supp_info: &Option<SupplementalInfo>,
) -> anyhow::Result<MediaFileInfo> {
    info!("Inspect: {}", qsf.file_path);
    let checksum_o = checksum_bytes(bytes).ok();
    let Some((short_checksum, long_checksum)) = checksum_o else {
        warn!("Could not calculate checksum for: {:?}", qsf.file_path);
        return Err(anyhow!(
            "Could not calculate checksum for file: {:?}",
            qsf.file_path
        ));
    };
    debug!("  Checksum calculated: {long_checksum}");
    if let Some(m) = all_media.get_mut(&long_checksum) {
        m.original_path.push(qsf.file_path.clone());
        return Ok(m.clone());
    }
    let media_file_info_res =
        media_file_info_from_readable(qsf, bytes, supp_info, &short_checksum, &long_checksum);
    let Ok(media_file) = media_file_info_res else {
        warn!("Could not calculate info for: {:?}", qsf.file_path);
        return Err(anyhow!("File type unsupported: {:?}", qsf.file_path));
    };
    all_media.insert(media_file.long_checksum.clone(), media_file.clone());
    Ok(media_file)
}

pub(crate) fn write_media(
    media_file: &MediaFileInfo,
    dry_run: bool,
    input_container: &mut Box<dyn PsContainer>,
    output_container: &mut PsDirectoryContainer,
) -> anyhow::Result<()> {
    info!("Output {:?}", media_file.desired_media_path);

    let desired_output_path_with_ext = match get_de_duplicated_path(media_file, output_container)? {
        SkipWrite => return Ok(()),
        WritePath(path) => path,
    };
    let bytes = input_container.file_bytes(&media_file.original_file_this_run)?;
    output_container.write(dry_run, &desired_output_path_with_ext.clone(), &bytes);
    output_container.set_modified(
        dry_run,
        &desired_output_path_with_ext.clone(),
        &media_file.modified,
    );
    Ok(())
}

#[derive(Debug, PartialEq)]
enum DeDuplicationResult {
    WritePath(String),
    SkipWrite,
}

fn get_de_duplicated_path(
    media_file: &MediaFileInfo,
    output_container: &mut PsDirectoryContainer,
) -> anyhow::Result<DeDuplicationResult> {
    let Some(desired_output_path) = &media_file.desired_media_path else {
        debug!("  No desired media path for file: {media_file:?}");
        return Err(anyhow!("No desired media path for file: {media_file:?}"));
    };
    for attempt in 0..MAX_DUPLICATE_CHECK_ATTEMPTS {
        let suffix = match attempt {
            0 => String::new(),
            // second to last attempt, use short checksum as suffix, should be mostly unique
            n if n == MAX_DUPLICATE_CHECK_ATTEMPTS - 2 => format!("-{}", media_file.short_checksum),
            // last attempt, use long checksum as suffix, should be guaranteed unique
            n if n == MAX_DUPLICATE_CHECK_ATTEMPTS - 1 => format!("-{}", media_file.long_checksum),
            // on first retry add -1, then -2, etc
            n => format!("-{n}"),
        };
        let desired_output_path_with_ext = format!(
            "{}{}.{}",
            desired_output_path, suffix, media_file.desired_media_extension
        );
        if !output_container.exists(&desired_output_path_with_ext) {
            return Ok(WritePath(desired_output_path_with_ext));
        }
        let es_o = is_existing_file_same(
            output_container,
            &media_file.long_checksum,
            &desired_output_path_with_ext,
        );
        match es_o {
            Some(true) => {
                debug!("  No need to write, file already exists with same checksum");
                return Ok(SkipWrite);
            }
            Some(false) => {
                warn!(
                    "  Existing file is different at {desired_output_path_with_ext}, attempting with different suffix"
                );
                // continue with next attempt
            }
            None => {
                warn!(
                    "  Could not determine if existing file is same or different {desired_output_path_with_ext}",
                );
                return Err(anyhow!(
                    "Could not determine if existing file is same or different: {desired_output_path:?}"
                ));
            }
        }
    }
    Err(anyhow!(format!(
        "Attempts to find a unique filename failed: {desired_output_path:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::PsDirectoryContainer;

    #[test]
    fn test_dedupe_one() -> anyhow::Result<()> {
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let mfi = MediaFileInfo::new_for_test(Some("duplicates/one".to_string()), "txt");
        let res = get_de_duplicated_path(&mfi, &mut c)?;
        assert_eq!(res, WritePath("duplicates/one-1.txt".to_string()));
        Ok(())
    }

    #[test]
    fn test_dedupe_many() -> anyhow::Result<()> {
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let mfi = MediaFileInfo::new_for_test(Some("duplicates/many".to_string()), "txt");
        let res = get_de_duplicated_path(&mfi, &mut c)?;
        assert_eq!(res, WritePath("duplicates/many-tsc.txt".to_string()));
        Ok(())
    }

    #[test]
    fn test_dedupe_too_many() -> anyhow::Result<()> {
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let mfi = MediaFileInfo::new_for_test(Some("duplicates/too-many".to_string()), "txt");
        let res = get_de_duplicated_path(&mfi, &mut c);
        assert_eq!(res.ok(), None);
        Ok(())
    }
}
