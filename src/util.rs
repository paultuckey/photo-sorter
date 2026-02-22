use crate::db_cmd::HashInfo;
use crate::file_type::{QuickFileType, find_quick_file_type};
use anyhow::anyhow;
use chrono::DateTime;
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::Path;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, warn};
use zip::ZipArchive;

/// Similar to github generate a short and long hash from the bytes
pub(crate) fn checksum_bytes<R: Read>(mut reader: R) -> anyhow::Result<HashInfo> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192]; // Read in 8KB chunks
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    let hex = hex::encode(digest);
    let chars = hex.chars();
    Ok(HashInfo {
        short_checksum: chars.clone().take(7).collect(),
        long_checksum: chars.take(64).collect(),
    })
}

pub(crate) trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

pub(crate) trait PsContainer {
    fn scan(&self) -> Vec<ScanInfo>;
    fn file_bytes(&mut self, path: &str) -> anyhow::Result<Vec<u8>>;
    fn file_reader(&mut self, path: &str) -> anyhow::Result<Box<dyn ReadSeek>>;
    fn exists(&self, path: &str) -> bool;
    fn root_exists(&self) -> bool;
}

#[derive(Debug, Clone)]
pub(crate) struct ScanInfo {
    pub(crate) file_path: String,
    /// Unix Epoch time of last file modification
    pub(crate) modified_datetime: Option<i64>,
    /// Unix Epoch time file creation
    pub(crate) created_datetime: Option<i64>,
    pub(crate) quick_file_type: QuickFileType,
}

impl ScanInfo {
    pub(crate) fn new(
        file_path: String,
        modified_datetime: Option<i64>,
        created_datetime: Option<i64>,
    ) -> Self {
        let quick_file_type = find_quick_file_type(&file_path);
        ScanInfo {
            file_path,
            modified_datetime,
            created_datetime,
            quick_file_type,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PsDirectoryContainer {
    root: String,
}

impl PsDirectoryContainer {
    pub(crate) fn new(root: &str) -> Self {
        PsDirectoryContainer {
            root: root.to_string(),
        }
    }
    pub(crate) fn write<R: Read>(&self, dry_run: bool, path: &String, reader: R) {
        let p = Path::new(&self.root).join(path);
        if dry_run {
            debug!("Dry run: would write file {:?}", p);
            return;
        }
        if let Err(e) = fs::create_dir_all(p.parent().unwrap()) {
            error!("Unable to create directory {:?}: {}", p.parent(), e);
            return;
        }
        let mut file = match File::create(&p) {
            Ok(f) => f,
            Err(e) => {
                error!("Unable to create file {p:?}: {e}");
                return;
            }
        };
        if let Err(e) = std::io::copy(&mut reader.take(u64::MAX), &mut file) {
            error!("Unable to write file {p:?}: {e}");
            return;
        }
        debug!("Wrote file {p:?}");
    }

    pub(crate) fn set_modified(
        &self,
        dry_run: bool,
        path: &String,
        modified_datetime: &Option<i64>,
    ) {
        let p = Path::new(&self.root).join(path);
        let Some(dt) = modified_datetime else {
            return;
        };
        let st = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_millis(*dt as u64))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if dry_run {
            debug!("  Dry run: would set modified datetime for file {p:?} to {dt}");
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

/// Recursively scans the directory and its subdirectories,
fn scan_dir_recursively(files: &mut Vec<ScanInfo>, dir_path: &Path, root_path: &Path) {
    if !dir_path.exists() || !dir_path.is_dir() {
        return;
    }
    let Ok(dir_reader) = fs::read_dir(dir_path) else {
        debug!("Unable to read directory: {dir_path:?}");
        return;
    };
    for dir_entry in dir_reader {
        let Ok(dir_entry) = dir_entry else {
            debug!("Unable to read directory entry");
            continue;
        };
        let path = dir_entry.path();
        if !path.exists() {
            continue;
        }
        if path.is_file() {
            // trim root path from the file path
            let relative_path = path.strip_prefix(root_path).unwrap_or(&path);
            files.push(ScanInfo::new(
                relative_path.to_string_lossy().to_string(),
                modified_time_ms(&path),
                created_time_ms(&path),
            ));
        } else if path.is_dir() {
            scan_dir_recursively(files, &path, root_path);
        }
    }
}

fn modified_time_ms(path: &Path) -> Option<i64> {
    path.metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|dt| {
            dt.duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as i64)
        })
}

fn created_time_ms(path: &Path) -> Option<i64> {
    path.metadata()
        .ok()
        .and_then(|m| m.created().ok())
        .and_then(|dt| {
            dt.duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as i64)
        })
}

impl PsContainer for PsDirectoryContainer {
    fn scan(&self) -> Vec<ScanInfo> {
        let mut files = Vec::new();
        let root_path = Path::new(&self.root);
        if !root_path.exists() {
            debug!("Root path does not exist: {root_path:?}");
            return files;
        }
        if !root_path.is_dir() {
            debug!("Root path is not a directory: {root_path:?}");
            return files;
        }
        scan_dir_recursively(&mut files, root_path, root_path);
        files
    }
    fn file_bytes(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
        let file_path = Path::new(&self.root).join(path);
        let file_r = File::open(&file_path);
        let Ok(mut file) = file_r else {
            debug!("Unable to open file: {file_path:?}");
            return Err(anyhow!("Unable to open file {file_path:?}"));
        };
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap_or(0);
        Ok(buffer)
    }

    fn file_reader(&mut self, path: &str) -> anyhow::Result<Box<dyn ReadSeek>> {
        let file_path = Path::new(&self.root).join(path);
        let file_r = File::open(&file_path);
        let Ok(file) = file_r else {
            warn!("Unable to open file: {file_path:?}");
            return Err(anyhow!("Unable to open file {file_path:?}"));
        };
        Ok(Box::new(file))
    }

    fn exists(&self, file: &str) -> bool {
        Path::new(&self.root).join(file).exists()
    }
    fn root_exists(&self) -> bool {
        Path::new(&self.root).exists()
    }
}

pub(crate) struct PsZipContainer {
    zip_file: String,
    index: Vec<ScanInfo>,
    zip: ZipArchive<File>,
    tz: chrono::FixedOffset,
}

impl PsZipContainer {
    pub(crate) fn new(zip_file: &String, tz: chrono::FixedOffset) -> Self {
        let z = ZipArchive::new(File::open(zip_file.clone()).unwrap());
        let mut c = PsZipContainer {
            zip_file: zip_file.to_string(),
            index: vec![],
            zip: z.unwrap(),
            tz,
        };
        c.index();
        c
    }
    fn index(&mut self) {
        let zip_archive = &mut self.zip;
        for i in 0..zip_archive.len() {
            let file_res = zip_archive.by_index(i);
            let Some(file_in_zip) = file_res.ok() else {
                continue;
            };
            if file_in_zip.is_dir() {
                continue;
            }
            let Some(enclosed_name) = file_in_zip.enclosed_name() else {
                continue;
            };
            let p = enclosed_name.as_path();
            let file_name_o = p.to_str();
            let Some(file_name) = file_name_o else {
                continue;
            };
            let mut dt_o = None;
            if let Some(lm) = file_in_zip.last_modified() {
                // not zip dates are dodgy, see zip crate's docs
                // zip times don't include tz, blindly assume they are in the local tz
                dt_o = chrono::NaiveDate::from_ymd_opt(
                    lm.year() as i32,
                    lm.month() as u32,
                    lm.day() as u32,
                )
                .and_then(|date| date.and_hms_opt(lm.hour() as u32, lm.minute() as u32, 0))
                .and_then(|naive_dt| naive_dt.and_local_timezone(self.tz).single())
                .map(|dt| dt.timestamp_millis());
            }
            self.index
                .push(ScanInfo::new(file_name.to_string(), dt_o, None));
        }
        debug!(
            "Counted {} files in zip {:?}",
            self.index.len(),
            self.zip_file
        );
    }
}

impl PsContainer for PsZipContainer {
    fn scan(&self) -> Vec<ScanInfo> {
        self.index.clone()
    }
    fn file_bytes(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
        let file_res = self.zip.by_name(path);
        let Some(mut file) = file_res.ok() else {
            warn!("Unable to open file: {path:?}");
            return Err(anyhow!("Unable to find file {:?}", path));
        };
        if file.is_dir() {
            warn!("Attempted roe read bytes from a directory: {path:?}");
            return Err(anyhow!("File is a dir {:?}", path));
        }
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap_or(0);
        Ok(buffer)
    }

    fn file_reader(&mut self, path: &str) -> anyhow::Result<Box<dyn ReadSeek>> {
        let bytes = self.file_bytes(path)?;
        Ok(Box::new(Cursor::new(bytes)))
    }

    fn exists(&self, path: &str) -> bool {
        self.index.iter().any(|i| i.file_path.eq(path))
    }
    fn root_exists(&self) -> bool {
        Path::new(&self.zip_file).exists()
    }
}

pub(crate) fn is_existing_file_same(
    output_container: &mut PsDirectoryContainer,
    long_checksum: &str,
    output_path: &String,
) -> Option<bool> {
    let Ok(reader) = output_container.file_reader(output_path) else {
        debug!("Could not read file bytes for checksum: {output_path:?}");
        return None;
    };
    let existing_file_hash_info_r = checksum_bytes(reader);
    let Ok(existing_file_hash_info) = existing_file_hash_info_r else {
        debug!("Could not read file for checksum: {output_path:?}");
        return None;
    };
    Some(existing_file_hash_info.long_checksum.eq(long_checksum))
}

pub(crate) fn dir_part(file_path_s: &String) -> String {
    let file_path = Path::new(&file_path_s);
    let Some(parent_path) = file_path.parent() else {
        warn!("No parent directory for file path: {file_path_s:?}");
        return "@@broken".to_string();
    };
    parent_path.to_string_lossy().to_string()
}

pub(crate) fn name_part(file_path_s: &String) -> String {
    let file_path = Path::new(&file_path_s);

    let Some(file_name_str) = file_path.file_name() else {
        warn!("No file name for file path: {file_path_s:?}");
        return "@@broken".to_string();
    };
    file_name_str.to_string_lossy().to_string()
}

pub(crate) fn timestamp_to_rfc3339(ts: i64) -> Option<String> {
    DateTime::from_timestamp_millis(ts).map(|d| d.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zip() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let c = PsZipContainer::new(&"test/Canon_40D.jpg.zip".to_string(), tz);
        let index = c.scan();
        assert_eq!(index.len(), 2);
        let si = index.first().unwrap();
        assert_eq!(si.file_path, "Canon_40D.jpg");
        assert_eq!(si.modified_datetime, Some(1749917340000));
        Ok(())
    }

    #[test]
    fn test_files_checksum() -> anyhow::Result<()> {
        use crate::util::PsDirectoryContainer;
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let b = c.file_reader("Canon_40D.jpg")?;
        let csm = checksum_bytes(b)?;
        assert_eq!(csm.short_checksum, "6bfdabd".to_string());
        assert_eq!(
            csm.long_checksum,
            "6bfdabd4fc33d112283c147acccc574e770bbe6fbdbc3d4da968ba7b606ecc2f".to_string()
        );
        Ok(())
    }
}
