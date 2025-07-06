use crate::file_type::{AccurateFileType, QuickFileType, QuickScannedFile};
use crate::util::{dir_part, name_part, PsContainer};
use std::collections::{HashMap, HashSet};
use std::io;
use serde_json::Value;
use log::{debug, info, warn};
use crate::media::MediaFileInfo;

///
/// Albums do not relate to a file they are in effect a back reference against the md file.
/// We also need to store the order.
/// This is done via a markdown file.
///
pub(crate) fn parse_csv_album(
    container: &mut Box<dyn PsContainer>,
    qsf: &QuickScannedFile,
) -> Option<Album> {
    let bytes_r = container.file_bytes(&qsf.name);
    let Ok(bytes) = bytes_r else {
        debug!("No bytes for album: {:?}", &qsf.name);
        return None;
    };
    parse_csv(&bytes, &qsf.name)
}


pub(crate) fn parse_json_album(container: &mut Box<dyn PsContainer>, qsf: &QuickScannedFile, all_media: &HashMap<String, MediaFileInfo>) -> Option<Album> {
    let bytes_r = container.file_bytes(&qsf.name);
    let Ok(bytes) = bytes_r else {
        debug!("No bytes for album: {:?}", &qsf.name);
        return None;
    };
    let j: Result<Value, _> = serde_json::from_slice(&bytes);
    let title;
    if let Ok(j) = j {
        let title_res = j.get("title");
        if let Some(title_value) = title_res {
            debug!("Found album title: {title_value:?}");
            title = Some(title_value.as_str().unwrap_or("").to_string());
        } else {
            debug!("");
            return None;
        }
    } else {
        warn!("Unable to decode album JSON: {:?}", &qsf.name);
        return None;
    }
    // all files in this directory are in the album
    let directory_path_str = dir_part(&qsf.name);
    // look up the media path in the media_path_map
    let same_dir_media_file_info = all_media.values()
        .filter(|m| {
            m.accurate_file_type != AccurateFileType::Unsupported &&
                m.quick_file_type == QuickFileType::Media &&
                m.original_path.iter().any(|p| {
                    let pp = dir_part(p);
                    pp.eq(&directory_path_str)
                })
        })
        .collect::<Vec<&MediaFileInfo>>();
    let same_dir_desired_paths = same_dir_media_file_info.iter()
        .filter_map(|m| m.desired_media_path.clone())
        .collect::<Vec<String>>();

    let directory_path_name_str = name_part(&directory_path_str);
    let desired_album_md_path = format!("albums/{directory_path_name_str}.md");
    // todo: how check for existing album?
    Some(Album {
        desired_album_md_path,
        title: title.unwrap_or(directory_path_name_str),
        files: same_dir_desired_paths,
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

fn parse_csv(bytes: &Vec<u8>, name: &String) -> Option<Album> {
    let cursor = io::Cursor::new(bytes);
    let mut rdr = csv::Reader::from_reader(cursor);
    let Ok(s) = rdr.headers() else {
        debug!("No headers");
        return None;
    };
    debug!("Headers: {s:?}");
    if s.is_empty() {
        debug!("No headers");
        return None;
    }
    let Some(col0) = s.get(0) else {
        debug!("No first header");
        return None;
    };
    if col0.trim().to_lowercase() != "imagename" {
        debug!("Not an icloud album");
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
        let Some(col0) = record.get(0) else {
            continue;
        };
        //if !is_file_media(&col0.to_string()) {
        //    debug!("Non media file: {:?}", col0);
        //    continue;
        //}
        files.push(col0.to_string());
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

pub(crate) fn build_album_md(album: &Album) -> String {
    let mut md = String::new();
    let generated_warning = "---\nphoto-lister:\n  album: This file is a GENERATED album, do NOT edit it directly.\n---\n\n";
    let media_relative_path = "../";

    md.push_str(generated_warning);
    md.push_str(&format!("# {}", &album.title));
    md.push_str("\n\n");
    for f in album.files.clone() {
        let alt_text = "Photo";
        let path = format!("{media_relative_path}{f}");
        md.push_str(&format!("\n![{alt_text}]({path})"));
    }
    md.push_str("\n\n");
    md
}
