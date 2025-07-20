use crate::file_type::{AccurateFileType, QuickFileType};
use crate::util::{dir_part, name_part, PsContainer, ScanInfo};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use serde_json::Value;
use log::{debug, info, warn};
use crate::media::MediaFileInfo;


pub(crate) fn parse_album(container: &mut Box<dyn PsContainer>,
                          si: &ScanInfo,
                          si_files: &[ScanInfo],
) -> Option<Album> {
    match si.quick_file_type {
        QuickFileType::AlbumCsv => {
            parse_csv_album(container, si)
        }
        QuickFileType::AlbumJson => {
            parse_json_album(container, si, si_files)
        }
        _ => {
            None
        }
    }
}

fn parse_csv_album(
    container: &mut Box<dyn PsContainer>,
    qsf: &ScanInfo,
) -> Option<Album> {
    info!("Parse CSV album: {:?}", &qsf.file_path);
    let bytes_r = container.file_bytes(&qsf.file_path);
    let Ok(bytes) = bytes_r else {
        warn!("No bytes for album: {:?}", &qsf.file_path);
        return None;
    };
    let name = &qsf.file_path;
    let cursor = io::Cursor::new(bytes);
    let mut rdr = csv::Reader::from_reader(cursor);
    let Ok(s) = rdr.headers() else {
        debug!("  No headers");
        return None;
    };
    if s.is_empty() {
        debug!("  Headers empty");
        return None;
    }
    let Some(col0) = s.get(0) else {
        debug!("  No first header");
        return None;
    };
    if col0.trim().to_lowercase() != "Images".to_lowercase() {
        debug!("  Not an iCloud album (column 0 should be 'Images', was {col0})");
        return None;
    }
    let mut files: Vec<String> = vec![];

    for result in rdr.records() {
        let Ok(record) = result else {
            debug!("Error reading record");
            continue;
        };
        debug!("{record:?}");
        if record.is_empty() {
            continue;
        }
        let Some(file_name) = record.get(0) else {
            continue;
        };

        // look for file with the original path {} + file_name
        let directory_path_str = dir_part(&qsf.file_path);

        let original_file = Path::new(&directory_path_str).join(file_name)
            .to_string_lossy().to_string();

        files.push(original_file);
    }
    if files.is_empty() {
        debug!("Not an album: {name:?}");
        return None;
    }
    // find index of last dot and get all chars before that
    let name_without_ext;
    let dot_idx = name.rfind('.').map_or(0, |idx| idx);
    if dot_idx > 0 {
        name_without_ext = name[..dot_idx].to_string();
        if name_without_ext.is_empty() {
            debug!("Album file has no name: {name:?}");
            return None;
        }
    } else {
        name_without_ext = name.clone();
        if name_without_ext.is_empty() {
            debug!("Album file has no name: {name:?}");
            return None;
        }
    }
    info!(
        "Found album: {:?} with {:?} entries at {:?}",
        name_without_ext,
        files.len(),
        name
    );
    Some(Album {
        title: name_without_ext.clone(),
        desired_album_md_path: name.clone(),
        files,
    })
}

fn parse_json_album(container: &mut Box<dyn PsContainer>, qsf: &ScanInfo, all_scanned_files: &[ScanInfo]) -> Option<Album> {
    let bytes_r = container.file_bytes(&qsf.file_path);
    let Ok(bytes) = bytes_r else {
        warn!("No bytes for album: {:?}", &qsf.file_path);
        return None;
    };
    let j: Result<Value, _> = serde_json::from_slice(&bytes);
    let title;
    if let Ok(j) = j {
        let title_res = j.get("title");
        if let Some(title_value) = title_res {
            debug!("  Found album title: {title_value}");
            title = Some(title_value.as_str().unwrap_or("").to_string());
        } else {
            warn!("Title not found in JSON, skipping {:?}", &qsf.file_path);
            return None;
        }
    } else {
        warn!("Unable to decode album JSON: {:?}", &qsf.file_path);
        return None;
    }
    // all files in this directory are in the album
    let directory_path_str = dir_part(&qsf.file_path);
    // look up the media path in the media_path_map
    let same_dir_files = all_scanned_files.iter()
        .filter(|si| {
            let q_dir_part = &dir_part(&si.file_path);
            si.quick_file_type == QuickFileType::Media && directory_path_str.eq(q_dir_part)
        })
        .map(|m| m.file_path.clone())
        .collect::<Vec<String>>();

    let directory_path_name_str = name_part(&directory_path_str);
    let desired_album_md_path = format!("albums/{directory_path_name_str}.md");
    // todo: how check for existing album?
    Some(Album {
        desired_album_md_path,
        title: title.unwrap_or(directory_path_name_str),
        files: same_dir_files,
    })
}

pub(crate) fn de_duplicate_albums(albums: &Vec<Album>) -> Vec<Album> {
    let mut clean_albums: Vec<Album> = vec![];
    let mut used_names = HashSet::new();
    for album in albums {
        let mut name = album.title.clone();
        let mut attempt = 0;
        loop {
            attempt += 1;
            if !used_names.contains(&name) {
                clean_albums.push(Album {
                    title: name.clone(),
                    desired_album_md_path: album.desired_album_md_path.clone(),
                    files: album.files.clone(),
                });
                used_names.insert(name);
                break;
            }
            name = format!("{name}-{attempt}");
            if attempt > 100 {
                warn!(
                    "Too many attempts to find unique name for album: {:?}",
                    &album.title
                );
                break;
            }
        }
    }
    clean_albums
}

pub(crate) struct Album {
    pub(crate) desired_album_md_path: String,
    pub(crate) title: String,
    files: Vec<String>,
}

/// albums to maps of media files with a vec of album names
pub(crate) fn albums_to_files_map(albums: &[Album]) -> HashMap<String, Vec<String>> {
    let mut m = HashMap::<String, Vec<String>>::new();
    for album in albums {
        for f in album.files.clone() {
            match m.get_mut(&f) {
                Some(v) => {
                    v.push(album.title.clone());
                }
                None => {
                    m.insert(f.clone(), vec![album.title.clone()]);
                }
            }
        }
    }
    m
}

pub(crate) fn build_album_md(album: &Album, all_media_o: Option<&HashMap<String, MediaFileInfo>>, media_relative_path: &str) -> String {
    let mut md = String::new();
    let generated_warning = "\n\n\n[ This file is a GENERATED album, do NOT edit it directly ]: #\n\n\n";
    // todo: in yaml front matter link back to original album
    md.push_str(generated_warning);
    md.push_str(&format!("# {}", &album.title));
    md.push_str("\n\n");
    for f in album.files.clone() {
        let alt_text = "Photo";
        let mut target_path_o = None;
        if let Some(all_media) = all_media_o {
            let media_file_info_o = all_media.values()
                .find(|m| {
                    m.accurate_file_type != AccurateFileType::Unsupported &&
                        m.quick_file_type == QuickFileType::Media &&
                        m.original_path.iter().any(|p| p.eq(&f))
                });
            let target_path_o = media_file_info_o
                .and_then(|m| m.desired_media_path.clone());
            if target_path_o.is_none() {
                warn!("No media file desired path found for: {f}");
                continue;
            }
        } else {
            // intentionally use the original path
            target_path_o = Some(f.clone());
        }
        if let Some(target_path) = target_path_o {
            let path = format!("{media_relative_path}{target_path}");
            md.push_str(&format!("\n![{alt_text}]({path})"));
        }
    }
    md.push_str("\n\n");
    md
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ic_sample() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        use crate::util::PsDirectoryContainer;
        let mut c: Box<dyn PsContainer> = Box::new(PsDirectoryContainer::new(&"test".to_string()));
        assert_eq!(c.root_exists(), true);
        let qsf = ScanInfo::new("ic-album-sample.csv".to_string(), None);
        let a = parse_album(&mut c, &qsf, &vec![]).unwrap();
        assert_eq!(a.title, "ic-album-sample".to_string());
        assert_eq!(a.files.len(), 5);
        assert_eq!(a.files.get(0).unwrap(), "35F8739B-30E0-4620-802C-0817AD7356F6.JPG");
        Ok(())
    }

    #[test]
    fn test_g_sample() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        use crate::util::PsDirectoryContainer;
        let mut c: Box<dyn PsContainer> = Box::new(PsDirectoryContainer::new(&"test/takeout1".to_string()));
        let qsf = ScanInfo::new("Google Photos/album1/metadata.json".to_string(), None);
        let si1 = ScanInfo::new("Google Photos/album1/test1.jpg".to_string(), None);
        let si2 = ScanInfo::new("different/test2.jpg".to_string(), None);
        let a = parse_album(&mut c, &qsf, &vec![si1, si2]).unwrap();
        assert_eq!(a.title, "Some album title".to_string());
        assert_eq!(a.files.len(), 1);
        assert_eq!(a.files.get(0).unwrap().to_string(), "Google Photos/album1/test1.jpg".to_string());
        Ok(())
    }
}
