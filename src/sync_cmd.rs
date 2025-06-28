use crate::album::{build_album_md, parse_csv_album, parse_json_album};
use crate::file_type::{QuickFileType, QuickScannedFile, quick_file_scan};
use crate::markdown_cmd::{assemble_markdown, mfm_from_media_file_info};
use crate::media::media_file_info_from_readable;
use crate::util::{PsContainer, PsDirectoryContainer, PsZipContainer, checksum_bytes};
use anyhow::anyhow;
use indicatif::ProgressBar;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{debug, info};

struct State {
    indexing_spinner: ProgressBar,
    supplemental_progress: ProgressBar,
    media_progress: ProgressBar,
    albums_progress: ProgressBar,
}

static UI_STATE: OnceLock<State> = OnceLock::new();

fn ui() -> &'static State {
    UI_STATE.get_or_init(|| State {
        indexing_spinner: ProgressBar::new_spinner(),
        supplemental_progress: ProgressBar::new(1),
        media_progress: ProgressBar::new(1),
        albums_progress: ProgressBar::new(1),
    })
}

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

    ui().indexing_spinner
        .enable_steady_tick(Duration::from_millis(100));
    ui().indexing_spinner.set_message("Indexing...");
    let files = container.scan();
    let quick_scanned_files = quick_file_scan(&container, &files);
    ui().indexing_spinner.finish_and_clear();
    info!("Indexed {} files in zip", files.len());

    let mut output_container: Option<PsDirectoryContainer> = None;
    if let Some(output) = output_directory {
        output_container = Some(PsDirectoryContainer::new(output.clone()));
    }

    if !skip_media {
        let supplemental_paths = quick_scanned_files
            .iter()
            .filter(|m| m.supplemental_json_file.is_some())
            .collect::<Vec<&QuickScannedFile>>();
        ui().supplemental_progress
            .set_length(supplemental_paths.len() as u64);
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
            ui().supplemental_progress.inc(1);
        }
        ui().supplemental_progress.finish_and_clear();
        info!("Read {} supplemental files", json_hashmap.len());

        let quick_media_files = quick_scanned_files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::Media)
            .collect::<Vec<&QuickScannedFile>>();
        ui().media_progress
            .set_length(quick_media_files.len() as u64);
        info!(
            "Inspecting {} photo and video files",
            quick_media_files.len()
        );
        for quick_scanned_file in quick_media_files {
            let bytes = container.file_bytes(&quick_scanned_file.name.clone());
            let Ok(bytes) = bytes else {
                debug!("Could not read file: {}", quick_scanned_file.name);
                return Err(anyhow!("Could not read file: {}", quick_scanned_file.name));
            };
            let _ = process(
                bytes,
                quick_scanned_file,
                dry_run,
                &mut output_container,
                skip_markdown,
                &json_hashmap,
            );
            ui().media_progress.inc(1);
        }
        ui().media_progress.finish_and_clear();
    }

    if !skip_albums {
        let csv_album_files = quick_scanned_files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::AlbumCsv)
            .collect::<Vec<&QuickScannedFile>>();
        let json_album_files = quick_scanned_files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::AlbumJson)
            .collect::<Vec<&QuickScannedFile>>();
        let total_album_files = csv_album_files.len() + json_album_files.len();
        ui().albums_progress.set_length(total_album_files as u64);
        info!("Inspecting {} albums", total_album_files);
        for csv_album in csv_album_files {
            let album_o = parse_csv_album(&mut container, csv_album);
            let Some(_) = album_o else {
                continue;
            };
            ui().albums_progress.inc(1);
        }
        for json_album in json_album_files {
            let album_o = parse_json_album(&mut container, json_album, &quick_scanned_files);
            if let Some(a) = album_o {
                let a_s = build_album_md(&a);
                if let Some(output_c) = &output_container {
                    let output_path = &a.desired_album_md_path;
                    if output_c.exists(&a.desired_album_md_path) {
                        debug!("Album markdown file already exists at {:?}", output_path);
                    } else {
                        if dry_run {
                            debug!("Would create album markdown file at {:?}", output_path);
                        } else {
                            let bytes = &a_s.as_bytes().to_vec();
                            output_c.write(dry_run, output_path, bytes);
                        }
                    }
                }
            };
            ui().albums_progress.inc(1);
        }
        ui().indexing_spinner.finish_and_clear();
        info!("Done albums");
    }

    Ok(())
}

pub(crate) fn process(
    bytes: Vec<u8>,
    qsf: &QuickScannedFile,
    dry_run: bool,
    output_container: &mut Option<PsDirectoryContainer>,
    skip_markdown: bool,
    extra_files: &HashMap<String, Vec<u8>>,
) -> anyhow::Result<()> {
    let file = &qsf.name;

    let extra_info_path = qsf.supplemental_json_file.clone();
    let mut extra_info_bytes: Option<Vec<u8>> = None;
    if let Some(path) = extra_info_path.clone() {
        if let Some(b) = extra_files.get(&path) {
            extra_info_bytes = Some(b.clone());
        } else {
            debug!("No extra info file found for: {:?}", path);
        }
    }

    let media_file_info_res = media_file_info_from_readable(&bytes, &qsf.name, &extra_info_bytes);
    let Ok(media_file) = media_file_info_res else {
        debug!("File type unsupported: {:?}", file);
        return Err(anyhow!("File type unsupported: {:?}", file));
    };

    if let Some(output_c) = output_container {
        if let Some(desired_output_path) = media_file.desired_media_path.clone() {
            let mut verified_output_path = None;
            if output_c.exists(&desired_output_path) {
                let eso =
                    is_existing_file_same(output_c, &media_file.checksum, &desired_output_path);
                if let Some(existing_same) = eso {
                    if existing_same {
                        // todo: check if output_path exists, if so, use checksum
                        verified_output_path = Some(desired_output_path.clone());
                    } else {
                        // todo: find another name
                        // todo: this will affect markdown path
                        debug!("Find another name {:?}", desired_output_path);
                    }
                }
            } else {
                verified_output_path = Some(desired_output_path.clone());
            }
            if let Some(verified_path) = verified_output_path {
                output_c.write(dry_run, &verified_path, &bytes);
            }
        }
        if !skip_markdown {
            if let Some(m) = media_file.desired_markdown_path.clone() {
                let output_path = m;
                let mfm = mfm_from_media_file_info(&media_file);
                let s = assemble_markdown(&mfm, "")?;
                let md_bytes = s.as_bytes().to_vec();
                if output_c.exists(&output_path) {
                    debug!("Markdown file already exists at {:?}", output_path);
                    // todo: grab existing content and discard frontmatter
                } else {
                    output_c.write(dry_run, &output_path, &md_bytes);
                }
            }
        }
    }
    Ok(())
}

fn is_existing_file_same(
    output_container: &mut PsDirectoryContainer,
    input_checksum: &Option<String>,
    output_path: &String,
) -> Option<bool> {
    debug!("Output path exists, check checksum {:?}", output_path);
    let Some(media_file_checksum) = input_checksum.clone() else {
        debug!("Media file does not have a checksum, cannot verify existing file");
        return None;
    };
    let Ok(bytes) = output_container.file_bytes(output_path) else {
        debug!("Could not read file for checksum: {:?}", output_path);
        return None;
    };
    let existing_file_checksum_r = checksum_bytes(&bytes);
    let Ok(existing_file_checksum) = existing_file_checksum_r else {
        debug!("Could not read file for checksum: {:?}", output_path);
        return None;
    };
    if !existing_file_checksum.eq(&media_file_checksum) {
        debug!("File exists but checksum does not match: {:?}", output_path);
        return Some(false);
    }
    debug!("File exists with matching checksum at {:?}", output_path);
    Some(true)
}
