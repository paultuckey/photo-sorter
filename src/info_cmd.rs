use crate::album::{build_album_md, parse_album};
use crate::file_type::QuickFileType;
use crate::fs::{FileSystem, OsFileSystem};
use crate::inspect::analyze_file;
use crate::markdown::{assemble_markdown, mfm_from_media_file_info};
use crate::util::{ScanInfo, scan_fs};
use tracing::{debug, warn};

pub(crate) fn main(input: &String, root_s: &str) -> anyhow::Result<()> {
    debug!("Inspecting: {input}");
    let root: Box<dyn FileSystem> = Box::new(OsFileSystem::new(root_s));
    let len = root.metadata(input).map(|m| m.len).unwrap_or(0);
    let si = ScanInfo::new(input.clone(), None, None, len);
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
    let Some(media_file_info) = analyze_file(root, si)? else {
        debug!("Not a valid media file: {}", si.file_path);
        return Ok(());
    };

    println!("Hash info:");
    println!(" short checksum: {}", media_file_info.hash_info.short_checksum);
    println!(" long checksum: {}", media_file_info.hash_info.long_checksum);

    println!("Markdown:");
    let mfm = mfm_from_media_file_info(&media_file_info);
    let s = assemble_markdown(&mfm, &None, "")?.into_string();
    println!("{s}");
    println!();

    if let Some(exif_info) = &media_file_info.exif_info {
        if !exif_info.tags.is_empty() {
            println!("EXIF:");
            for (tn, tv) in &exif_info.tags {
                println!("  {tn}: {tv}");
            }
        }
        if let Some(gps) = &exif_info.gps {
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

    // The markdown links to the media's original paths (see `build_album_md`'s
    // `None` branch), so there's no need to inspect/hash the referenced media.
    debug!("Markdown:");
    let md = build_album_md(&album, None, "", None);
    println!("{md}");

    Ok(())
}
