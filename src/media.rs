use crate::exif::{ParsedExif, best_guess_taken_dt, parse_exif};
use crate::file_type::{AccurateFileType, determine_file_type, file_ext_from_file_type};
use crate::util::{PsReadable, checksum_from_read, PsContainer};
use anyhow::anyhow;
use chrono::{DateTime, Datelike, Timelike};
use std::ffi::OsStr;
use std::path::Path;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub(crate) struct MediaFileInfo {
    pub(crate) original_path: String,
    pub(crate) file_format: AccurateFileType,
    pub(crate) parsed_exif: Option<ParsedExif>,
    pub(crate) checksum: Option<String>,
    pub(crate) desired_media_path: Option<String>,
    pub(crate) desired_markdown_path: Option<String>,
    pub(crate) extra_info: Option<String>,
}

pub(crate) fn media_file_info_from_readable(
    container: &Box<dyn PsContainer>,
    reader: &dyn PsReadable,
    extra_info_path: &Option<String>,
) -> anyhow::Result<MediaFileInfo> {
    let input = reader.name();
    let guessed_ff = determine_file_type(reader);
    if guessed_ff == AccurateFileType::Unsupported {
        debug!("File {:?} is not a valid media file", input);
        return Err(anyhow!("File is not a valid media file"));
    }
    let exif_o = parse_exif(reader, &guessed_ff);
    let checksum_o = checksum_from_read(reader).ok();

    let ext = file_ext_from_file_type(&guessed_ff);

    let guessed_datetime = best_guess_taken_dt(&exif_o);
    let mut desired_media_path_o = None;
    let mut desired_markdown_path_o = None;
    match checksum_o.clone() {
        Some(checksum) => {
            desired_media_path_o = Some(get_desired_media_path(
                &checksum.clone(),
                &guessed_datetime,
                &ext,
                0,
            ));
            desired_markdown_path_o = get_desired_markdown_path(desired_media_path_o.clone());
        }
        None => {
            // could not calculate checksum, not a valid file
        }
    }
    let mut extra_info = None;
    if let Some(extra_info_path) = extra_info_path {
        let c = container.readable(extra_info_path);
        let bytes_res = c.to_bytes();
        if let Ok(bytes) = bytes_res {
            debug!(
                "Extra info file {:?} has {} bytes",
                extra_info_path,
                bytes.len()
            );
            let string = String::from_utf8_lossy(&bytes);
            extra_info = Some(string.trim().to_string());
        } else {
            debug!(
                "Could not read extra info file {:?} relating to {:?}",
                extra_info_path, input
            );
        }
    }
    let media_file_info = MediaFileInfo {
        original_path: input.clone(),
        file_format: guessed_ff.clone(),
        parsed_exif: exif_o.clone(),
        checksum: checksum_o.clone(),
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
            .map_or(None, |(name, _)| Some(name.to_string() + ".md")),
    }
}

/// `yyyy/mm/dd/hhmm-ss[-i].ext`
/// OR `undated/checksum.ext`
pub(crate) fn get_desired_media_path(
    checksum: &String,
    exif_datetime: &Option<String>,
    ext: &String,
    de_dupe_int: u16,
) -> String {
    let date_dir;
    let name;
    if let Some(dt_s) = exif_datetime {
        let dt = DateTime::parse_from_rfc3339(dt_s);
        match dt {
            Ok(dt) => {
                date_dir = format!("{}/{:0>2}/{:0>2}", dt.year(), dt.month(), dt.day());
                name = format!("{:0>2}{:0>2}-{:0>2}", dt.hour(), dt.minute(), dt.second());
            }
            Err(e) => {
                warn!("Could not parse EXIF datetime: {:?}", e);
                date_dir = "undated".to_string();
                name = checksum.to_owned();
            }
        }
    } else {
        date_dir = "undated".to_string();
        name = checksum.to_owned();
    }
    let mut de_dupe_str = "".to_string();
    if de_dupe_int > 0 {
        de_dupe_str = format!("-{}", de_dupe_int);
    }
    format!("{date_dir}/{}{}.{}", name, de_dupe_str, ext)
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
