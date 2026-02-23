use crate::db_cmd::HashInfo;
use crate::file_type::{QuickFileType, find_quick_file_type};
use crate::fs::{FileSystem, OsFileSystem};
use anyhow::Result;
use chrono::DateTime;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use tracing::{debug, warn};

/// Similar to github generate a short and long hash from the bytes
pub(crate) fn checksum_bytes<R: Read>(mut reader: R) -> Result<HashInfo> {
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

#[derive(Debug, Clone)]
pub(crate) struct ScanInfo {
    pub(crate) file_path: String,
    /// Unix Epoch time of last file modification
    pub(crate) modified_datetime: Option<i64>,
    /// Unix Epoch time file creation
    pub(crate) created_datetime: Option<i64>,
    pub(crate) file_size: u64,
    pub(crate) quick_file_type: QuickFileType,
}

impl ScanInfo {
    pub(crate) fn new(
        file_path: String,
        modified_datetime: Option<i64>,
        created_datetime: Option<i64>,
        file_size: u64,
    ) -> Self {
        let quick_file_type = find_quick_file_type(&file_path);
        ScanInfo {
            file_path,
            modified_datetime,
            created_datetime,
            file_size,
            quick_file_type,
        }
    }
}

pub(crate) fn scan_fs(fs: &dyn FileSystem) -> Vec<ScanInfo> {
    let paths = fs.walk();
    let mut scan_infos = Vec::new();
    for path in paths {
        let meta = fs.metadata(&path).ok();
        let (mod_dt, create_dt, len) = match meta {
            Some(m) => (m.modified, m.created, m.len),
            None => (None, None, 0),
        };
        scan_infos.push(ScanInfo::new(path, mod_dt, create_dt, len));
    }
    scan_infos
}

pub(crate) fn is_existing_file_same(
    fs: &OsFileSystem,
    long_checksum: &str,
    output_path: &String,
) -> Option<bool> {
    let Ok(reader) = fs.open(output_path) else {
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
    use crate::fs::ZipFileSystem;

    #[test]
    fn test_zip() -> anyhow::Result<()> {
        use anyhow::anyhow;
        crate::test_util::setup_log();
        let tz =
            chrono::FixedOffset::east_opt(0).ok_or_else(|| anyhow!("Failed to create timezone"))?;
        let c = ZipFileSystem::new("test/Canon_40D.jpg.zip", tz)?;
        let index = scan_fs(&c);
        assert_eq!(index.len(), 2);
        // Find Canon_40D.jpg
        let si = index
            .iter()
            .find(|i| i.file_path == "Canon_40D.jpg")
            .ok_or_else(|| anyhow!("Canon_40D.jpg not found in zip"))?;
        assert_eq!(si.modified_datetime, Some(1749917340000));
        Ok(())
    }

    #[test]
    fn test_files_checksum() -> anyhow::Result<()> {
        let c = OsFileSystem::new("test");
        let b = c.open("Canon_40D.jpg")?;
        let csm = checksum_bytes(b)?;
        assert_eq!(csm.short_checksum, "6bfdabd".to_string());
        assert_eq!(
            csm.long_checksum,
            "6bfdabd4fc33d112283c147acccc574e770bbe6fbdbc3d4da968ba7b606ecc2f".to_string()
        );
        Ok(())
    }
}
