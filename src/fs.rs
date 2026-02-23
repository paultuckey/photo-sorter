use anyhow::{anyhow, Result};
use chrono::FixedOffset;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tracing::{debug, error};
use zip::ZipArchive;

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    #[allow(dead_code)]
    pub len: u64,
    #[allow(dead_code)]
    pub is_dir: bool,
    pub modified: Option<i64>,
    pub created: Option<i64>,
}

pub trait FileSystem: Send + Sync {
    fn open(&self, path: &str) -> Result<Box<dyn ReadSeek>>;
    fn exists(&self, path: &str) -> bool;
    // Walk returns all files recursively as relative paths
    fn walk(&self) -> Vec<String>;
    fn metadata(&self, path: &str) -> Result<FileMetadata>;
}

#[derive(Debug, Clone)]
pub struct OsFileSystem {
    root: PathBuf,
}

impl OsFileSystem {
    pub fn new(root: &str) -> Self {
        Self {
            root: PathBuf::from(root),
        }
    }

    pub fn root_exists(&self) -> bool {
        self.root.exists()
    }

    pub fn write<R: Read>(&self, dry_run: bool, path: &str, mut reader: R) {
        let p = self.root.join(path);
        if dry_run {
            debug!("Dry run: would write file {:?}", p);
            return;
        }
        if let Some(parent) = p.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            error!("Unable to create directory {:?}: {}", parent, e);
            return;
        }
        let mut file = match File::create(&p) {
            Ok(f) => f,
            Err(e) => {
                error!("Unable to create file {p:?}: {e}");
                return;
            }
        };
        if let Err(e) = std::io::copy(&mut reader, &mut file) {
            error!("Unable to write file {p:?}: {e}");
            return;
        }
        debug!("Wrote file {p:?}");
    }

    pub fn set_modified(&self, dry_run: bool, path: &str, modified_datetime: &Option<i64>) {
        let p = self.root.join(path);
        let Some(dt) = modified_datetime else {
            return;
        };
        let st = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_millis(*dt as u64))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if dry_run {
            debug!(
                "  Dry run: would set modified datetime for file {p:?} to {dt}"
            );
            return;
        }
        let f_r = File::open(&p);
        let Ok(f) = f_r else {
            error!("Unable to open file {p:?} for setting modified datetime ");
            return;
        };
        if let Err(e) = f.set_modified(st) {
            error!("Unable to set modified datetime for file {p:?}: {e}");
        } else {
            debug!("Set modified datetime for file {p:?} to {dt}");
        }
    }
}

impl FileSystem for OsFileSystem {
    fn open(&self, path: &str) -> Result<Box<dyn ReadSeek>> {
        let p = self.root.join(path);
        let f = File::open(&p).map_err(|e| anyhow!("Unable to open file {:?}: {}", p, e))?;
        Ok(Box::new(f))
    }

    fn exists(&self, path: &str) -> bool {
        self.root.join(path).exists()
    }

    fn walk(&self) -> Vec<String> {
        let mut files = Vec::new();
        if !self.root.exists() || !self.root.is_dir() {
            return files;
        }
        scan_dir_recursively(&mut files, &self.root, &self.root);
        files
    }

    fn metadata(&self, path: &str) -> Result<FileMetadata> {
        let p = self.root.join(path);
        let m = fs::metadata(&p)?;
        Ok(FileMetadata {
            len: m.len(),
            is_dir: m.is_dir(),
            modified: m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64),
            created: m
                .created()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64),
        })
    }
}

fn scan_dir_recursively(files: &mut Vec<String>, dir_path: &Path, root_path: &Path) {
    if !dir_path.exists() || !dir_path.is_dir() {
        return;
    }
    let Ok(dir_reader) = fs::read_dir(dir_path) else {
        debug!("Unable to read directory: {dir_path:?}");
        return;
    };
    for dir_entry in dir_reader {
        let Ok(dir_entry) = dir_entry else {
            continue;
        };
        let path = dir_entry.path();
        if path.is_file() {
            // trim root path from the file path
            let relative_path = path.strip_prefix(root_path).unwrap_or(&path);
            files.push(relative_path.to_string_lossy().to_string());
        } else if path.is_dir() {
            scan_dir_recursively(files, &path, root_path);
        }
    }
}

pub struct ZipFileSystem {
    #[allow(dead_code)]
    zip_file: String,
    zip: Mutex<ZipArchive<File>>,
    #[allow(dead_code)]
    tz: FixedOffset,
    file_names: Vec<String>,
    metadata_cache: HashMap<String, FileMetadata>,
}

impl ZipFileSystem {
    pub fn new(zip_file: &str, tz: FixedOffset) -> Result<Self> {
        let f = File::open(zip_file)?;
        let mut zip = ZipArchive::new(f)?;
        let mut file_names = Vec::new();
        let mut metadata_cache = HashMap::new();

        for i in 0..zip.len() {
            let Ok(file) = zip.by_index(i) else {
                continue;
            };
            if file.is_dir() {
                continue;
            }
            let Some(enclosed_name) = file.enclosed_name() else {
                continue;
            };
            let Some(name) = enclosed_name.to_str() else {
                continue;
            };
            let name_s = name.to_string();
            file_names.push(name_s.clone());

            let mut modified = None;
            if let Some(lm) = file.last_modified() {
                modified = chrono::NaiveDate::from_ymd_opt(
                    lm.year() as i32,
                    lm.month() as u32,
                    lm.day() as u32,
                )
                .and_then(|date| date.and_hms_opt(lm.hour() as u32, lm.minute() as u32, 0))
                .and_then(|naive_dt| naive_dt.and_local_timezone(tz).single())
                .map(|dt| dt.timestamp_millis());
            }

            metadata_cache.insert(
                name_s,
                FileMetadata {
                    len: file.size(),
                    is_dir: false,
                    modified,
                    created: None,
                },
            );
        }
        Ok(Self {
            zip_file: zip_file.to_string(),
            zip: Mutex::new(zip),
            tz,
            file_names,
            metadata_cache,
        })
    }
}

impl FileSystem for ZipFileSystem {
    fn open(&self, path: &str) -> Result<Box<dyn ReadSeek>> {
        let mut zip = self.zip.lock().map_err(|e| anyhow!("Zip lock failed: {}", e))?;
        let mut file = zip
            .by_name(path)
            .map_err(|_| anyhow!("File not found in zip: {}", path))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(Box::new(Cursor::new(buffer)))
    }

    fn exists(&self, path: &str) -> bool {
        self.metadata_cache.contains_key(path)
    }

    fn walk(&self) -> Vec<String> {
        self.file_names.clone()
    }

    fn metadata(&self, path: &str) -> Result<FileMetadata> {
        self.metadata_cache
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow!("File not found in zip metadata cache: {}", path))
    }
}
