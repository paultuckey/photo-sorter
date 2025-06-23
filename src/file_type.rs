use crate::extra_info::detect_extra_info;
use crate::util::PsContainer;
use tracing::debug;
use tracing::log::warn;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum QuickFileType {
    Media,
    Album,
    Unknown,
}

pub(crate) fn find_quick_file_type(file_name: &str) -> QuickFileType {
    let ext_s = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    let ext = ext_s.as_str();
    // todo: not supported gif
    match ext {
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "mp4" => QuickFileType::Media,
        "csv" => QuickFileType::Album,
        _ => QuickFileType::Unknown,
    }
}

pub(crate) struct QuickScannedFile {
    pub(crate) name: String,
    pub(crate) quick_file_type: QuickFileType,
    pub(crate) supplemental_json_file: Option<String>,
}

pub(crate) fn quick_file_scan(
    container: &Box<dyn PsContainer>,
    files: Vec<String>,
) -> Vec<QuickScannedFile> {
    debug!("Scanning {} files for quick file type", files.len());
    let mut scanned_files = vec![];
    for file in &files {
        let qft = find_quick_file_type(file);
        match qft {
            QuickFileType::Media => {
                let scanned_file = QuickScannedFile {
                    name: file.clone(),
                    quick_file_type: qft,
                    supplemental_json_file: detect_extra_info(&file.clone(), container),
                };
                scanned_files.push(scanned_file);
            }
            QuickFileType::Album => {
                let scanned_file = QuickScannedFile {
                    name: file.clone(),
                    quick_file_type: qft,
                    supplemental_json_file: None,
                };
                scanned_files.push(scanned_file);
            }
            QuickFileType::Unknown => {
                continue;
            }
        }
    }
    scanned_files
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
        debug!("mime type:{:?} file:{:?} ", name, mt);
        return mt;
    }
    // Limit buffer size same as that inside `file_format` crate
    // let buffer_res = media_file_readable.take(36_870);
    if bytes.is_empty() {
        warn!("file is empty file:{:?}", name);
        return AccurateFileType::Unsupported;
    };
    let fmt = file_format::FileFormat::from_bytes(bytes);
    let mt = fmt.media_type();
    if mt == "application/octet-stream" {
        debug!("can not guess mime type file:{:?}", name);
        return AccurateFileType::Unsupported;
    }
    if mt == "application/x-empty" {
        debug!("file appears to be empty file:{:?}", name);
        return AccurateFileType::Unsupported;
    }
    debug!("mime type:{:?} file:{:?} ", name, mt);
    file_type_from_content_type(mt)
}

#[tokio::test()]
async fn test_quick_file_type() {
    crate::test_util::setup_log().await;
    assert_eq!(find_quick_file_type("test/test1.jpg"), QuickFileType::Media);
    assert_eq!(find_quick_file_type("test/test1.mp4"), QuickFileType::Media);
    assert_eq!(
        find_quick_file_type("test/test1.abc"),
        QuickFileType::Unknown
    );
    assert_eq!(find_quick_file_type("test/test1.csv"), QuickFileType::Album);
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
