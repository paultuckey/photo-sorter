use std::collections::HashMap;
use std::path::Path;
use anyhow::{anyhow, Context};
use log::{debug, warn};
use crate::album::{build_album_md, parse_album};
use crate::supplemental_info::{detect_supplemental_info, load_supplemental_info};
use crate::sync_cmd::inspect_media;
use crate::util::{PsContainer, PsDirectoryContainer, ScanInfo};

pub(crate) async fn main(input: &String) -> anyhow::Result<()> {
    debug!("Inspecting: {input}");
    let p = Path::new(input);
    let parent_dir = p
        .parent() //
        .with_context(|| "Unable to get parent directory")?;
    let parent_dir_string = parent_dir.to_string_lossy().to_string();
    let mut root: Box<dyn PsContainer> = Box::new(PsDirectoryContainer::new(parent_dir_string));
    let si = ScanInfo::new(input.clone(), None);
    let files = root.scan();

    let album_o = parse_album(&mut root, &si, &files);
    let Some(album) = album_o else {
        warn!("Not a valid album file: {input}");
        return Ok(());
    };
    let mut all_media = HashMap::new();
    files
        .iter()
        .filter(|f| f.quick_file_type == crate::file_type::QuickFileType::Media)
        .for_each(|f| {
            let mut si_o = None;
            let sp_o = detect_supplemental_info(&f.file_path.clone(), &mut root);
            if let Some(sp) = sp_o {
                si_o = load_supplemental_info(&sp, &mut root);
            }
            let bytes = root.file_bytes(&si.file_path.clone());
            let Ok(bytes) = bytes else {
                warn!("Could not read file: {}", si.file_path);
                return;
            };
            let _ = inspect_media(bytes, f, &mut all_media, &si_o);
        });
    build_album_md(&album, &all_media);

    Ok(())
}