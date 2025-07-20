use crate::exif::{ParsedExif, best_guess_taken_dt, parse_exif};
use crate::file_type::{AccurateFileType, determine_file_type, file_ext_from_file_type, QuickFileType};
use anyhow::anyhow;
use chrono::{DateTime, Datelike, Timelike};
use log::{warn};
use crate::supplemental_info::SupplementalInfo;
use crate::util::ScanInfo;

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
    pub(crate) supp_info: Option<SupplementalInfo>,
    // Modified date in RFC3339 format
    pub(crate) modified: Option<i64>,
}

pub(crate) fn media_file_info_from_readable(
    qsf: &ScanInfo,
    bytes: &Vec<u8>,
    supp_info: &Option<SupplementalInfo>,
    short_checksum: &String,
    long_checksum: &String,
) -> anyhow::Result<MediaFileInfo> {
    let name = &qsf.file_path;
    let guessed_ff = determine_file_type(bytes, name);
    if guessed_ff == AccurateFileType::Unsupported {
        warn!("Not a valid media file {name:?}");
        return Err(anyhow!("File is not a valid media file"));
    }
    let exif_o = parse_exif(bytes, name, &guessed_ff);

    let ext = file_ext_from_file_type(&guessed_ff);
    let guessed_datetime = best_guess_taken_dt(&exif_o, &qsf.modified_datetime, &supp_info);
    let desired_media_path_o = Some(get_desired_media_path(
        &short_checksum.clone(),
        &guessed_datetime,
        &ext,
    ));
    let desired_markdown_path_o = get_desired_markdown_path(desired_media_path_o.clone());

    let media_file_info = MediaFileInfo {
        original_path: vec![name.clone()],
        accurate_file_type: guessed_ff.clone(),
        quick_file_type: qsf.quick_file_type.clone(),
        parsed_exif: exif_o.clone(),
        short_checksum: short_checksum.clone(),
        long_checksum: long_checksum.clone(),
        desired_media_path: desired_media_path_o.clone(),
        desired_markdown_path: desired_markdown_path_o.clone(),
        supp_info: supp_info.clone(),
        modified: qsf.modified_datetime,
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
    media_datetime: &Option<i64>,
    ext: &String,
) -> String {
    let date_dir;
    let name;
    if let Some(dt_ms) = media_datetime {
        let dt = DateTime::from_timestamp_millis(dt_ms.clone());
        match dt {
            Some(dt) => {
                date_dir = format!("{}/{:0>2}/{:0>2}", dt.year(), dt.month(), dt.day());
                let time_name = format!("{:0>2}{:0>2}-{:0>2}", dt.hour(), dt.minute(), dt.second());
                name = format!("{time_name}-{short_checksum}");
            }
            None => {
                warn!("Could not parse datetime: {dt_ms:?}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desired_md_path() {
        crate::test_util::setup_log();
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

    #[test]
    fn test_desired_path() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        use crate::util::PsDirectoryContainer;
        use crate::util::PsContainer;
        use crate::util::{checksum_bytes};

        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let bytes = c.file_bytes(&"Canon_40D.jpg".to_string()).unwrap();
        let short_checksum = checksum_bytes(&bytes)?.0;

        assert_eq!(get_desired_media_path(&short_checksum, &None, &"jpeg".to_string()),
                   "undated/6bfdabd.jpeg".to_string());
        assert_eq!(get_desired_media_path(&short_checksum, &Some(1212162961000), &"jpeg".to_string()),
                   "2008/05/30/1556-01-6bfdabd.jpeg".to_string());
        Ok(())
    }
}