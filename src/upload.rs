use std::collections::HashMap;
use std::fs::read_dir;
use std::path::{Path, PathBuf};

use crate::util::checksum_file;
use anyhow::Context;
use futures::stream::StreamExt;
use std::fs;
use std::time::Instant;
use chrono::{DateTime, Datelike, Timelike};
use tracing::{debug, error, info, warn};

use serde_json::Value;
use crate::album::{albums_to_files_map, detect_albums, is_file_maybe_album, upload_albums, de_duplicate_albums};
use crate::exif_util::is_file_media;

#[derive(Debug, Clone)]
pub(crate) struct FsFile {
    pub(crate) path: String,
    pub(crate) rel_path: String,
}

pub(crate) async fn upload(paths: &Vec<String>) -> anyhow::Result<()> {
    let num_ingested = ingest_now(paths).await?;
    info!("Ingested {} files", num_ingested);
    Ok(())
}

async fn ingest_now(directory_paths: &Vec<String>) -> anyhow::Result<u32> {
    let now = Instant::now();
    let mut files: Vec<FsFile> = vec![];
    let mut total_files = 0;
    let mut files_done = 0;

    let add_file = &mut |f: FsFile| {
        files.push(f);
        total_files += 1;
    };

    for directory_string in directory_paths {
        let directory_path = Path::new(directory_string).to_path_buf();
        index_media(&directory_path, add_file);
    }

    //let thread_tx = tx.clone();
    let mut files_to_upload: Vec<FsFile> = vec![];
    let mut files_maybe_album: Vec<FsFile> = vec![];
    for f in files {
        let file_path = Path::new(&f.path);
        info!("File: {:?}", file_path);
        if is_file_maybe_album(&f.path.to_string()) {
            files_maybe_album.push(f.clone());
        } else if is_file_media(&f.path.to_string()) {
            files_to_upload.push(f.clone());
            // if files_to_upload.len() > 100 {
            //     break;
            // }
            files_done += 1;
            continue;
        }
        files_done += 1;
    };

    info!("Total files: {:?}", total_files);

    info!("maybe albums: {:?}", files_maybe_album.len());
    let mut albums = detect_albums(files_maybe_album).await?;
    albums = de_duplicate_albums(&albums);
    let files_with_albums = albums_to_files_map(&albums);

    info!("files to upload: {:?}", files_to_upload.len());

    //upload_files(config, client, local_store, &files_to_upload, &files_with_albums).await?;
    //upload_albums(config, client, local_store, &albums).await?;

    let elapsed = now.elapsed();
    info!("Elapsed: {:.2?}", elapsed);
    Ok(total_files)
}

fn visit_dir(
    _base_path: &PathBuf,
    current_path: &PathBuf,
    cb: &mut dyn FnMut(FsFile),
) {
    debug!("Visit dir: {:?}", current_path);
    match read_dir(current_path) {
        Ok(rd) => {
            for e in rd {
                match e {
                    Ok(e) => {
                        let path = e.path();
                        if path.is_dir() {
                            visit_dir(_base_path, &path, cb);
                        } else if path.is_file() {
                            //info!("{:?}", path);
                            let rel_path = get_relative_path(_base_path, &path);
                            if let Some(rp) = rel_path {
                                // todo {imgbasename}.supplemental-metadata.json
                                cb(FsFile {
                                    path: path.display().to_string(),
                                    rel_path: rp.display().to_string(),
                                });
                            } else {
                                warn!("Error getting relative path: {:?}", path);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error reading directory entry {}", e);
                        return;
                    }
                }
            }
        }
        Err(e) => {
            error!("Error reading directory {}", e);
        }
    }
}

fn index_media(
    base_path: &PathBuf,
    cb: &mut dyn FnMut(FsFile),
) {
    let base_path = Path::new(base_path).to_path_buf();
    if base_path.exists() && base_path.is_dir() {
        visit_dir(&base_path, &base_path, cb);
    } else {
        error!("Directory invalid: {:?}", fs::canonicalize(base_path));
    }
}

fn get_relative_path(base: &PathBuf, file: &Path) -> Option<PathBuf> {
    file.strip_prefix(base).map(|p| p.to_path_buf()).ok()
}

async fn upload_files(files_to_upload: &[FsFile], albums: &HashMap<String, Vec<String>>) -> anyhow::Result<()> {
    let num_threads = 4;
    let fetches = futures::stream::iter(
        files_to_upload.iter().map(|ff| {
            let album_names = albums.get(&ff.rel_path).cloned();
            async move {
                match media_all(ff, album_names).await {
                    Ok(_) => {}
                    Err(e) => warn!("ERROR uploading {}: {:?}", ff.path, e),
                }
            }
        })
    ).buffer_unordered(num_threads).collect::<Vec<()>>();
    info!("Waiting...");
    fetches.await;
    Ok(())
}

async fn media_all(ff: &FsFile, albums: Option<Vec<String>>) -> anyhow::Result<()> {
    info!("Starting {}", ff.path);
    let file_path = Path::new(&ff.path);
    let checksum = checksum_file(file_path)?;

    info!("Uploading blob {}", ff.path);
    // upload_blob(&upload_url, &ff.path).await?;
    // let d = upload_done(&upload_id, ff).await?;
    // info!("Done {}, media_id: {:?}, status: {:?}", ff.path, d.media_id, d.status);
    Ok(())
}


pub(crate) struct MediaStartRequest {
    pub(crate) checksum: String,
    pub(crate) albums: Option<Vec<String>>,
}

pub(crate) struct MediaStartResponse {
    pub(crate) upload_id: Option<String>,
    pub(crate) upload_url: Option<String>,
    pub(crate) media_id: Option<String>,
}




async fn upload_blob(_: &String, _: &String) -> anyhow::Result<()> {
    // let byte_buf: Vec<u8> = fs::read(file_path)?;

    Ok(())
}

pub(crate) struct MediaDoneRequest {
    pub(crate) upload_id: String,
    pub(crate) from: String,
    pub(crate) extra_info: Option<String>,
}

pub(crate) struct MediaDoneResponse {
    pub(crate) status: String,
    pub(crate) media_id: Option<String>,
}

pub(crate) async fn upload_done(upload_id: &str, ff: &FsFile) -> anyhow::Result<()> {
    let extra_info_s = detect_extra_info(ff).await?;
    // let body = serde_json::to_string(&MediaDoneRequest {
    //     upload_id: upload_id.to_owned(),
    //     from: ff.rel_path.to_owned(),
    //     extra_info: extra_info_s,
    // }).with_context(|| "Unable to encode send media list")?;
    Ok(())
}

async fn detect_extra_info(ff: &FsFile) -> anyhow::Result<Option<String>> {
    let google_supp_json_exts = vec![
        ".supplemental-metadata.json",
        ".supplemental-metad.json",
        ".suppl.json",
    ];
    for supp_json_ext in google_supp_json_exts {
        let n = format!("{}{}", &ff.path, supp_json_ext);
        let supp_info_path = Path::new(&n);
        if supp_info_path.exists() {
            debug!("Found google supplemental metadata file: {:?}", supp_info_path);
            let s = fs::read_to_string(supp_info_path)
                .with_context(|| format!("Unable to read file: {:?}", supp_info_path))?;
            let j: Result<Value, _> = serde_json::from_str(&s);
            if let Ok(j) = j {
                let c = serde_json::to_string(&j);
                if let Ok(c) = c {
                    info!("Found supplemental metadata: {:?}", supp_info_path);
                    return Ok(Some(c));
                } else {
                    warn!("Unable to encode extra info JSON: {:?}", supp_info_path);
                    // continue and try others
                }
            } else {
                warn!("Unable to decode extra info JSON: {:?}", supp_info_path);
                // continue and try others
            }
        }
    }
    Ok(None)
}
