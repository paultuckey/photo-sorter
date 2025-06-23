use crate::file_type::QuickScannedFile;
use crate::util::{PsContainer};
use std::collections::{HashMap, HashSet};
use std::io;
use tracing::{debug, info, warn};

///
/// Albums do not relate to a file they are in effect a back reference against the md file.
/// We also need to store the order.
/// This is done via a markdown file.
///
pub(crate) fn detect_album(
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

pub(crate) fn de_duplicate_albums(albums: &Vec<Album>) -> Vec<Album> {
    let mut clean_albums: Vec<Album> = vec![];
    let mut used_names = HashSet::new();
    for album in albums {
        let mut name = album.name.clone();
        let mut attempt = 0;
        loop {
            attempt += 1;
            if !used_names.contains(&name) {
                clean_albums.push(Album {
                    name: name.clone(),
                    path: album.path.clone(),
                    files: album.files.clone(),
                });
                used_names.insert(name);
                break;
            }
            name = format!("{}-{}", name, attempt);
            if attempt > 100 {
                warn!(
                    "Too many attempts to find unique name for album: {:?}",
                    &album.name
                );
                break;
            }
        }
    }
    clean_albums
}

pub(crate) struct Album {
    path: String,
    pub(crate) name: String,
    files: Vec<String>,
}

fn parse_csv(bytes: &Vec<u8>, name: &String) -> Option<Album> {
    let cursor = io::Cursor::new(bytes);
    let mut rdr = csv::Reader::from_reader(cursor);
    let Ok(s) = rdr.headers() else {
        debug!("No headers");
        return None;
    };
    debug!("Headers: {:?}", s);
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
        debug!("{:?}", record);
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
        debug!("Not an album: {:?}", name);
        return None;
    }
    // find index of last dot and get all chars before that
    let name_without_ext;
    let dot_idx = name.rfind('.').map_or(0, |idx| idx);
    if dot_idx > 0 {
        name_without_ext = name[..dot_idx].to_string();
        if name_without_ext.is_empty() {
            debug!("Album file has no name: {:?}", name);
            return None;
        }
    } else {
        name_without_ext = name.clone();
        if name_without_ext.is_empty() {
            debug!("Album file has no name: {:?}", name);
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
        name: name_without_ext.clone(),
        path: name.clone(),
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
                    v.push(album.name.clone());
                }
                None => {
                    m.insert(f.clone(), vec![album.name.clone()]);
                }
            }
        }
    }
    m
}

pub(crate) fn build_album_md(album: &Album) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {}", &album.name));
    md.push_str("");
    for f in album.files.clone() {
        md.push_str(&format!("\n- [{}]({})", f, f));
    }
    md.push_str("");
    md
}
