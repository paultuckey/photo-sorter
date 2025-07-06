use crate::exif::{ParsedExif, best_guess_taken_dt, parse_exif};
use crate::file_type::{AccurateFileType, determine_file_type, file_ext_from_file_type, QuickFileType, QuickScannedFile};
use anyhow::anyhow;
use chrono::{DateTime, Datelike, Timelike};
use log::{debug, warn};

#[derive(Debug, Clone)]
pub(crate) struct MediaFileInfo {
    pub(crate) original_path: Vec<String>,
    pub(crate) desired_media_path: Option<String>,
    pub(crate) desired_markdown_path: Option<String>,
    pub(crate) quick_file_type: QuickFileType,
    pub(crate) parsed_exif: Option<ParsedExif>,
    pub(crate) accurate_file_type: AccurateFileType,
    pub(crate) short_checksum: String,
    pub(crate) long_checksum: String,
    pub(crate) extra_info: Option<String>,
}

pub(crate) fn media_file_info_from_readable(
    qsf: &QuickScannedFile,
    bytes: &Vec<u8>,
    extra_info_bytes: &Option<Vec<u8>>,
    short_checksum: &String,
    long_checksum: &String,
) -> anyhow::Result<MediaFileInfo> {
    let name = &qsf.name;
    let guessed_ff = determine_file_type(bytes, name);
    if guessed_ff == AccurateFileType::Unsupported {
        debug!("File {name:?} is not a valid media file");
        return Err(anyhow!("File is not a valid media file"));
    }
    let exif_o = parse_exif(bytes, name, &guessed_ff);

    let ext = file_ext_from_file_type(&guessed_ff);

    let guessed_datetime = best_guess_taken_dt(&exif_o);
    let desired_media_path_o = Some(get_desired_media_path(
        &short_checksum.clone(),
        &guessed_datetime,
        &ext,
    ));
    let desired_markdown_path_o = get_desired_markdown_path(desired_media_path_o.clone());

    let mut extra_info = None;
    if let Some(extra_info_bytes) = extra_info_bytes {
        debug!("Extra info file has {} bytes", extra_info_bytes.len());
        let string = String::from_utf8_lossy(extra_info_bytes);
        extra_info = Some(string.trim().to_string());
    }
    let media_file_info = MediaFileInfo {
        original_path: vec![name.clone()],
        accurate_file_type: guessed_ff.clone(),
        quick_file_type: qsf.quick_file_type.clone(),
        parsed_exif: exif_o.clone(),
        short_checksum: short_checksum.clone(),
        long_checksum: long_checksum.clone(),
        desired_media_path: desired_media_path_o.clone(),
        desired_markdown_path: desired_markdown_path_o.clone(),
        extra_info,
    };
    Ok(media_file_info)
}

pub(crate) fn get_desired_markdown_path(desired_media_path: Option<String>) -> Option<String> {
    match desired_media_path {
        None => None,
        Some(dmp) => dmp
            .rsplit_once('.')
            .map(|(name, _)| name.to_string() + ".md"),
    }
}

/// `yyyy/mm/dd/hhmm-ss[-i].ext`
/// OR `undated/checksum.ext`
pub(crate) fn get_desired_media_path(
    short_checksum: &String,
    exif_datetime: &Option<String>,
    ext: &String,
) -> String {
    let date_dir;
    let name;
    if let Some(dt_s) = exif_datetime {
        let dt = DateTime::parse_from_rfc3339(dt_s);
        match dt {
            Ok(dt) => {
                date_dir = format!("{}/{:0>2}/{:0>2}", dt.year(), dt.month(), dt.day());
                let time_name = format!("{:0>2}{:0>2}-{:0>2}", dt.hour(), dt.minute(), dt.second());
                name = format!("{time_name}-{short_checksum}");
            }
            Err(e) => {
                warn!("Could not parse EXIF datetime: {e:?}");
                date_dir = "undated".to_string();
                name = short_checksum.to_owned();
            }
        }
    } else {
        date_dir = "undated".to_string();
        name = short_checksum.to_owned();
    }
    format!("{date_dir}/{name}.{ext}")
}


#[tokio::test()]
async fn test_desired_md_path() {
    crate::test_util::setup_log().await;
    assert_eq!(get_desired_markdown_path(None), None);
    assert_eq!(
        get_desired_markdown_path(Some("abc.jpg".to_string())),
        Some("abc.md".to_string())
    );
    assert_eq!(get_desired_markdown_path(Some("abc".to_string())), None);
    assert_eq!(
        get_desired_markdown_path(Some("abc.def.ghi.jkl".to_string())),
        Some("abc.def.ghi.md".to_string())
    );
}

#[tokio::test()]
async fn test_desired_path() -> anyhow::Result<()> {
    crate::test_util::setup_log().await;
    use crate::util::PsDirectoryContainer;
    use crate::util::PsContainer;
    use crate::util::{checksum_bytes};

    let mut c = PsDirectoryContainer::new("test".to_string());
    let bytes = c.file_bytes(&"Canon_40D.jpg".to_string()).unwrap();
    let short_checksum = checksum_bytes(&bytes)?.0;

    assert_eq!(get_desired_media_path(&short_checksum, &None, &"jpeg".to_string()),
               "undated/6bfdabd.jpeg".to_string());
    assert_eq!(get_desired_media_path(&short_checksum, &Some("2017-08-19T10:21:59Z".to_string()), &"jpeg".to_string()),
               "2017/08/19/1021-59-6bfdabd.jpeg".to_string());
    Ok(())
}

