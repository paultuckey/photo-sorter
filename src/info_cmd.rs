use crate::album::{build_album_md, parse_album};
use crate::exif_util::parse_exif_info;
use crate::file_type::QuickFileType;
use crate::markdown::{assemble_markdown, mfm_from_media_file_info};
use crate::media::media_file_info_from_readable;
use crate::fs::{FileSystem, OsFileSystem};
use crate::supplemental_info::{detect_supplemental_info, load_supplemental_info};
use crate::sync_cmd::inspect_media;
use crate::util::{ScanInfo, checksum_bytes, scan_fs};
use anyhow::anyhow;
use std::collections::HashMap;
use tracing::{debug, warn};

pub(crate) fn main(input: &String, root_s: &str) -> anyhow::Result<()> {
    debug!("Inspecting: {input}");
    let root: Box<dyn FileSystem> = Box::new(OsFileSystem::new(root_s));
    let si = ScanInfo::new(input.clone(), None, None);
    match si.quick_file_type {
        QuickFileType::Unknown => {
            warn!("File type is unknown, skipping: {input}");
            Ok(())
        }
        QuickFileType::AlbumCsv | QuickFileType::AlbumJson => album(&si, root.as_ref()),
        QuickFileType::Media => media(&si, root.as_ref()),
    }
}

pub(crate) fn media(si: &ScanInfo, root: &dyn FileSystem) -> anyhow::Result<()> {
    let reader = root.open(&si.file_path.to_string())?;
    let hash_info_o = checksum_bytes(reader).ok();
    let Some(hash_info) = hash_info_o else {
        debug!("Could not calculate checksum for file: {:?}", si.file_path);
        return Err(anyhow!(
            "Could not calculate checksum for file: {:?}",
            si.file_path
        ));
    };

    let mut supp_info_o = None;
    let supp_info_path_o = detect_supplemental_info(&si.file_path.clone(), root);
    if let Some(supp_info_path) = supp_info_path_o {
        supp_info_o = load_supplemental_info(&supp_info_path, root);
    }
    let media_file_info_res = media_file_info_from_readable(si, root, &supp_info_o, &hash_info);
    let Ok(media_file_info) = media_file_info_res else {
        debug!("Not a valid media file: {}", si.file_path);
        return Ok(());
    };

    println!("Hash info:");
    println!(" short checksum: {}", hash_info.short_checksum);
    println!(" long checksum: {}", hash_info.long_checksum);

    println!("Markdown:");
    let mfm = mfm_from_media_file_info(&media_file_info);
    let s = assemble_markdown(&mfm, &None, "")?.into_string();
    println!("{s}");
    println!();

    let reader = root.open(&si.file_path.to_string())?;
    let exif_info_o = parse_exif_info(reader);
    if let Some(exif_info) = exif_info_o {
        if !exif_info.tags.is_empty() {
            println!("EXIF:");
            for (tn, tv) in exif_info.tags {
                println!("  {tn}: {tv}");
            }
        }
        if let Some(gps) = exif_info.gps {
            println!("EXIF:");
            println!("  gps: {gps}");
        }
    }
    println!();
    println!();
    Ok(())
}

pub(crate) fn album(si: &ScanInfo, root: &dyn FileSystem) -> anyhow::Result<()> {
    let files = scan_fs(root);
    let album_o = parse_album(root, si, &files);
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
            let _ = inspect_media(f, root, &mut all_media, &si_o);
        });

    debug!("Markdown:");
    let md = build_album_md(&album, None, "", None);
    println!("{md}");

    Ok(())
}
