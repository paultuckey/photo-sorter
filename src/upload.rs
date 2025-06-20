use std::fs::read_dir;
use std::path::{Path, PathBuf};

use std::fs;
use tracing::{debug, error, warn};

#[derive(Debug, Clone)]
pub(crate) struct FsFile {
    pub(crate) path: String,
    pub(crate) rel_path: String,
}

fn visit_dir(_base_path: &PathBuf, current_path: &PathBuf, cb: &mut dyn FnMut(FsFile)) {
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

fn index_media(base_path: &PathBuf, cb: &mut dyn FnMut(FsFile)) {
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
