use std::path::Path;
use crate::supplemental_info::detect_supplemental_info;
use crate::util::{PsContainer, ScanInfo};
use log::{debug, warn};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum QuickFileType {
    Media,
    AlbumCsv,
    AlbumJson,
    Unknown,
}

pub(crate) fn find_quick_file_type(file_path: &str) -> QuickFileType {
    let p = Path::new(file_path);
    let lowercase_file_name_str = p.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if lowercase_file_name_str.eq("metadata.json") {
        return QuickFileType::AlbumJson;
    }
    let lowercase_file_ext = p.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    match lowercase_file_ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "mp4" => QuickFileType::Media,
        "csv" => QuickFileType::AlbumCsv,
        _ => QuickFileType::Unknown,
    }
}

pub(crate) struct QuickScannedFile {
    pub(crate) name: String,
    /// rfc3339 formatted datetime of the last modification
    pub(crate) modified_datetime: Option<String>,
    pub(crate) quick_file_type: QuickFileType,
    pub(crate) supplemental_json_file: Option<String>,
}

impl QuickScannedFile {
    pub(crate) fn new(name: String, quick_file_type: QuickFileType, modified_datetime: Option<String>) -> Self {
        QuickScannedFile {
            name,
            modified_datetime,
            quick_file_type,
            supplemental_json_file: None,
        }
    }
}

pub(crate) async fn quick_scan_files(
    container: &Box<dyn PsContainer>,
    files: &Vec<ScanInfo>,
) -> Vec<QuickScannedFile> {
    debug!("Scanning {} files for quick file type", files.len());
    let mut scanned_files = vec![];
    for si in files {
        let Some(qsf) = quick_scan_file(container, si).await else {
            continue;
        };
        scanned_files.push(qsf);
    }
    scanned_files
}

pub(crate) async fn quick_scan_file(container: &Box<dyn PsContainer>, si: &ScanInfo) -> Option<QuickScannedFile> {
    let qft = find_quick_file_type(&si.file_path);
    match qft {
        QuickFileType::Media => {
            Some(QuickScannedFile {
                name: si.file_path.clone(),
                modified_datetime: si.modified_datetime.clone(),
                quick_file_type: qft,
                supplemental_json_file: detect_supplemental_info(&si.file_path.clone(), container),
            })
        }
        QuickFileType::AlbumCsv | QuickFileType::AlbumJson => {
            Some(QuickScannedFile::new(si.file_path.clone(), qft, si.modified_datetime.clone()))
        }
        QuickFileType::Unknown => {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AccurateFileType {
    Jpg,
    Png,
    Heic,
    Gif,
    Mp4,
    Json,
    Csv,
    Unsupported,
}

pub(crate) fn file_ext_from_file_type(ff: &AccurateFileType) -> String {
    match ff {
        AccurateFileType::Jpg => "jpg".to_string(),
        AccurateFileType::Gif => "gif".to_string(),
        AccurateFileType::Png => "png".to_string(),
        AccurateFileType::Heic => "heic".to_string(),
        AccurateFileType::Mp4 => "mp4".to_string(),
        AccurateFileType::Unsupported => "bin".to_string(),
        AccurateFileType::Json => "json".to_string(),
        AccurateFileType::Csv => "csv".to_string(),
    }
}

pub(crate) fn file_type_from_content_type(ct: &str) -> AccurateFileType {
    match ct {
        "image/jpeg" => AccurateFileType::Jpg,
        "image/gif" => AccurateFileType::Gif,
        "image/png" => AccurateFileType::Png,
        "image/heic" => AccurateFileType::Heic,
        "video/mp4" => AccurateFileType::Mp4,
        "application/octet-stream" => AccurateFileType::Unsupported,
        "application/json" => AccurateFileType::Unsupported,
        "text/csv" => AccurateFileType::Csv,
        _ => AccurateFileType::Unsupported,
    }
}

pub(crate) fn determine_file_type(bytes: &Vec<u8>, name: &String) -> AccurateFileType {
    // take json files at face value
    if name.to_lowercase().ends_with(".json") {
        let mt = AccurateFileType::Json;
        debug!("  mime type:{mt:?}");
        return mt;
    }
    // Limit buffer size same as that inside `file_format` crate
    // let buffer_res = media_file_readable.take(36_870);
    if bytes.is_empty() {
        warn!("  file is empty file:{name:?}");
        return AccurateFileType::Unsupported;
    };
    let fmt = file_format::FileFormat::from_bytes(bytes);
    let mt = fmt.media_type();
    if mt == "application/octet-stream" {
        debug!("  can not calculate mime type file:{name:?}");
        return AccurateFileType::Unsupported;
    }
    if mt == "application/x-empty" {
        debug!("  file appears to be empty file:{name:?}");
        return AccurateFileType::Unsupported;
    }
    debug!("  mime type {mt:?}");
    file_type_from_content_type(mt)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test()]
    async fn test_quick_file_type() {
        crate::test_util::setup_log().await;
        assert_eq!(find_quick_file_type("test/test1.jpg"), QuickFileType::Media);
        assert_eq!(find_quick_file_type("test/test1.mp4"), QuickFileType::Media);
        assert_eq!(
            find_quick_file_type("test/test1.abc"),
            QuickFileType::Unknown
        );
        assert_eq!(find_quick_file_type("test/test1.csv"), QuickFileType::AlbumCsv);
        assert_eq!(find_quick_file_type("test/test1.CsV"), QuickFileType::AlbumCsv);
        assert_eq!(find_quick_file_type("test/metadata.json"), QuickFileType::AlbumJson);
        assert_eq!(find_quick_file_type("test/MeTaDaTa.JsOn"), QuickFileType::AlbumJson);
        assert_eq!(find_quick_file_type("test/tes"), QuickFileType::Unknown);
        assert_eq!(find_quick_file_type("test/te.s.jpg"), QuickFileType::Media);
    }

    #[tokio::test()]
    async fn test_accurate_file_type() {
        crate::test_util::setup_log().await;
        use crate::util::PsDirectoryContainer;
        let name = "Canon_40D.jpg".to_string();
        let mut root = PsDirectoryContainer::new("test".to_string());
        let bytes = root.file_bytes(&name).unwrap();
        assert_eq!(determine_file_type(&bytes, &name), AccurateFileType::Jpg);

        let bad: Vec<u8> = vec![];
        assert_eq!(
            determine_file_type(&bad, &"bad.bad".to_string()),
            AccurateFileType::Unsupported
        );
    }
}