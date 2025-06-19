use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::Path;
use tracing::{debug, info, warn};
use crate::exif_util::is_file_media;
use crate::upload::FsFile;

/// Albums do not relate to a file they are in effect a back reference against the md file. 
/// We also need to store the order.
/// This is done via a markdown file.   
pub(crate) async fn detect_albums(maybe_album_files: Vec<FsFile>) -> anyhow::Result<Vec<Album>> {
    let mut albums: Vec<Album> = vec![];
    for ff in maybe_album_files {
        let a = parse_csv(&ff);
        let Some(album) = a else {
            continue;
        };
        albums.push(album);
    }
    Ok(albums)
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
                    files: album.files.clone()
                });
                used_names.insert(name);
                break;
            }
            name = format!("{}-{}", name, attempt);
            if attempt > 100 {
                warn!("Too many attempts to find unique name for album: {:?}", &album.name);
                break;
            }
        }
    }
    clean_albums
}

pub(crate) struct Album {
    path: String,
    name: String,
    files: Vec<String>,
}

fn parse_csv(ff: &FsFile) -> Option<Album> {
    let p = Path::new(&ff.path);
    let rdr = csv::Reader::from_path(p);
    let Ok(mut rdr) = rdr else {
        return None;
    };
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
        if !is_file_media(&col0.to_string()) {
            debug!("Non media file: {:?}", col0);
            continue;
        }
        files.push(col0.to_string());
    }
    if files.is_empty() {
        debug!("Not an album: {:?}", &ff.path);
        return None;
    }
    let name = p.file_stem()
        .and_then(OsStr::to_str)
        .map(|name| name.to_string())
        .unwrap_or(ff.path.clone());
    info!("Found album: {:?} with {:?} entries at {:?}", name, files.len(), &ff.path);
    Some(Album {
        name,
        path: ff.path.clone(),
        files,
    })
}

pub(crate) fn is_file_maybe_album(path: &String) -> bool {
    let p = Path::new(path);
    let ext = p.extension()
        .and_then(OsStr::to_str);
    let Some(ext) = ext else {
        debug!("No extension");
        return false;
    };
    let file_extensions = ["csv"];
    file_extensions.contains(&ext.to_string().to_ascii_lowercase().as_str())
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
                    m.insert(f.clone(), vec!(album.name.clone()));
                }
            }
        }
    }
    m
}

pub(crate) async fn upload_albums(albums_to_upload: &[Album]) -> anyhow::Result<()> {
    for album in albums_to_upload {
        let md = build_md(album);
        import_markdown(album, &md).await?;
    }
    Ok(())
}

pub(crate) fn build_md(album: &Album) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {}", &album.name));
    md.push_str("");
    for f in album.files.clone() {
        md.push_str(&format!("\n- [{}]({})", f, f));
    }
    md.push_str("");
    md
}

pub(crate) async fn import_markdown(_: &Album, _: &String) -> anyhow::Result<()> {
    // let body = serde_json::to_string(&ImportMarkdownRequest {
    //     file_path: album.path.to_owned(),
    //     markdown: md.to_string(),
    // }).with_context(|| "Unable to encode send media list")?;
    Ok(())
}

pub(crate) struct ImportMarkdownRequest {
    pub(crate) file_path: String,
    pub(crate) markdown: String,
}

pub(crate) struct ImportMarkdownResponse {}