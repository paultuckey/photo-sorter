use crate::album::detect_album;
use crate::file_type::{QuickFileType, QuickScannedFile, quick_file_scan};
use crate::markdown_cmd::{assemble_markdown, mfm_from_media_file_info};
use crate::media::{MediaFileInfo, media_file_info_from_readable};
use crate::util::{PsContainer, PsDirectoryContainer, PsZipContainer};
use anyhow::anyhow;
use indicatif::ProgressBar;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::Path;
use std::{fs, time::Duration};
use tracing::{debug, info};

struct State {
    indexing_spinner: ProgressBar,
    supplemental_progress: ProgressBar,
    media_progress: ProgressBar,
    albums_progress: ProgressBar,
}

static UI: Lazy<State> = Lazy::new(|| State {
    indexing_spinner: ProgressBar::new_spinner(),
    supplemental_progress: ProgressBar::new(1),
    media_progress: ProgressBar::new(1),
    albums_progress: ProgressBar::new(1),
});

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
    
    UI.indexing_spinner.enable_steady_tick(Duration::from_millis(100));
    UI.indexing_spinner.set_message("Indexing...");
    let files = container.scan();
    let quick_scanned_files = quick_file_scan(&container, &files);
    UI.indexing_spinner.finish_and_clear();
    info!("Indexed {} files in zip", files.len());
    
    if !skip_media {
        let supplemental_paths = quick_scanned_files
            .iter()
            .filter(|m| m.supplemental_json_file.is_some())
            .collect::<Vec<&QuickScannedFile>>();
        UI.supplemental_progress.set_length(supplemental_paths.len() as u64);
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
            UI.supplemental_progress.inc(1);
        }
        UI.supplemental_progress.finish_and_clear();
        info!("Read {} supplemental files", json_hashmap.len());

        let quick_media_files = quick_scanned_files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::Media)
            .collect::<Vec<&QuickScannedFile>>();
        UI.media_progress.set_length(quick_media_files.len() as u64);
        info!("Inspecting {} photo and video files", quick_media_files.len());
        for quick_scanned_file in quick_media_files {
            let bytes = container.file_bytes(&quick_scanned_file.name.clone());
            let Ok(bytes) = bytes else {
                debug!("Could not read file: {}", quick_scanned_file.name);
                return Err(anyhow!("Could not read file: {}", quick_scanned_file.name));
            };
            let _ = analyze(
                bytes,
                quick_scanned_file,
                dry_run,
                output_directory,
                skip_markdown,
                &json_hashmap,
            );
            UI.media_progress.inc(1);
        }
        UI.media_progress.finish_and_clear();
    }

    if !skip_albums {
        // in google takeout /{name}/metadata.json -> {title:"album name"}
        let quick_album_files = quick_scanned_files
            .iter()
            .filter(|m| m.quick_file_type == QuickFileType::Album)
            .collect::<Vec<&QuickScannedFile>>();
        UI.albums_progress.set_length(quick_album_files.len() as u64);
        info!("Inspecting {} albums", quick_album_files.len());
        for quick_album in quick_album_files {
            let album_o = detect_album(&mut container, quick_album);
            let Some(_) = album_o else {
                continue;
            };
            UI.albums_progress.inc(1);
        }
        UI.indexing_spinner.finish_and_clear();
        info!("Done albums");
    }

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
                let s = assemble_markdown(&mfm, "")?;
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
