use std::collections::HashMap;
use crate::media::{media_file_info_from_readable};
use crate::util::{PsContainer, PsDirectoryContainer, checksum_bytes, ScanInfo};
use anyhow::{anyhow, Context};
use log::{debug, warn};
use crate::album::{build_album_md, parse_album};
use crate::file_type::QuickFileType;
use crate::markdown::{assemble_markdown, mfm_from_media_file_info};
use crate::supplemental_info::{detect_supplemental_info, load_supplemental_info};
use crate::sync_cmd::inspect_media;

pub(crate) async fn main(input: &String, root_s: &String) -> anyhow::Result<()> {
    debug!("Inspecting: {input}");
    let mut root: Box<dyn PsContainer> = Box::new(PsDirectoryContainer::new(root_s));
    let si = ScanInfo::new(input.clone(), None);
    match si.quick_file_type {
        QuickFileType::Unknown => {
            warn!("File type is unknown, skipping: {input}");
            Ok(())
        }
        QuickFileType::AlbumCsv | QuickFileType::AlbumJson => {
            album(&si, &mut root).await
        }
        QuickFileType::Media => {
            media(&si, &mut root).await
        }
    }
}

pub(crate) async fn media(si: &ScanInfo, root: &mut Box<dyn PsContainer>) -> anyhow::Result<()> {
    let bytes = root
        .file_bytes(&si.file_path.to_string()) //
        .with_context(|| "Error reading media file")?;
    let checksum_o = checksum_bytes(&bytes).ok();
    let Some((short_checksum, long_checksum)) = checksum_o else {
        debug!("Could not calculate checksum for file: {:?}", si.file_path);
        return Err(anyhow!("Could not calculate checksum for file: {:?}", si.file_path));
    };
    let mut supp_info_o = None;
    let supp_info_path_o = detect_supplemental_info(&si.file_path.clone(), root);
    if let Some(supp_info_path) = supp_info_path_o {
        supp_info_o = load_supplemental_info(&supp_info_path, root);
    }
    let media_file_info_res = media_file_info_from_readable(
        &si, &bytes, &supp_info_o, &short_checksum, &long_checksum);
    let Ok(media_file_info) = media_file_info_res else {
        debug!("Not a valid media file: {}", si.file_path);
        return Ok(());
    };
    debug!("Markdown:");
    let mfm = mfm_from_media_file_info(&media_file_info);
    let s = assemble_markdown(&mfm, &None, "")?;
    println!("{s}");
    Ok(())
}


pub(crate) async fn album(si: &ScanInfo, root: &mut Box<dyn PsContainer>) -> anyhow::Result<()> {
    let files = root.scan();
    let album_o = parse_album(root, &si, &files);
    let Some(album) = album_o else {
        warn!("Not a valid album file: {}", si.file_path);
        return Ok(());
    };
    let mut all_media = HashMap::new();
    files
        .iter()
        .filter(|f| f.quick_file_type == QuickFileType::Media)
        .for_each(|f| {
            let mut si_o = None;
            let sp_o = detect_supplemental_info(&f.file_path.clone(), root);
            if let Some(sp) = sp_o {
                si_o = load_supplemental_info(&sp, root);
            }
            let bytes = root.file_bytes(&si.file_path.clone());
            let Ok(bytes) = bytes else {
                warn!("Could not read file: {}", si.file_path);
                return;
            };
            let _ = inspect_media(bytes, f, &mut all_media, &si_o);
        });

    debug!("Markdown:");
    let md = build_album_md(&album, None, "");
    println!("{md}");

    Ok(())
}

