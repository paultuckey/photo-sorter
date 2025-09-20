use crate::exif::{ParsedExif, best_guess_taken_dt, parse_exif};
use crate::file_type::{
    AccurateFileType, QuickFileType, determine_file_type, file_ext_from_file_type,
};
use crate::supplemental_info::SupplementalInfo;
use crate::util::ScanInfo;
use anyhow::anyhow;
use chrono::{DateTime, Datelike, Timelike};
use log::warn;

#[derive(Debug, Clone)]
pub(crate) struct MediaFileInfo {
    pub(crate) original_file_this_run: String,
    pub(crate) original_path: Vec<String>,
    /// Desired path relative to output directory, minus the dot and file extension (eg, 2025/09/10/1234-56-789)
    pub(crate) desired_media_path: Option<String>,
    /// Desired file extension (eg, jpg, mp4)
    pub(crate) desired_media_extension: String,
    pub(crate) quick_file_type: QuickFileType,
    pub(crate) parsed_exif: Option<ParsedExif>,
    pub(crate) accurate_file_type: AccurateFileType,
    pub(crate) short_checksum: String,
    pub(crate) long_checksum: String,
    pub(crate) supp_info: Option<SupplementalInfo>,
    // Modified date in RFC3339 format
    pub(crate) modified: Option<i64>,
    pub(crate) guessed_datetime: Option<i64>,
}

pub(crate) fn media_file_info_from_readable(
    scan_info: &ScanInfo,
    bytes: &Vec<u8>,
    supp_info: &Option<SupplementalInfo>,
    short_checksum: &str,
    long_checksum: &str,
) -> anyhow::Result<MediaFileInfo> {
    let name = &scan_info.file_path;
    let guessed_ff = determine_file_type(bytes, name);
    if guessed_ff == AccurateFileType::Unsupported {
        warn!("Not a valid media file {name:?}");
        return Err(anyhow!("File is not a valid media file"));
    }
    let exif_o = parse_exif(bytes, name, &guessed_ff);

    let ext = file_ext_from_file_type(&guessed_ff);
    let guessed_datetime = best_guess_taken_dt(&exif_o, scan_info, supp_info);
    let desired_media_path_o = Some(get_desired_media_path(short_checksum, &guessed_datetime));
    let media_file_info = MediaFileInfo {
        original_file_this_run: name.clone(),
        original_path: vec![name.clone()],
        accurate_file_type: guessed_ff.clone(),
        quick_file_type: scan_info.quick_file_type.clone(),
        parsed_exif: exif_o.clone(),
        short_checksum: short_checksum.to_string(),
        long_checksum: long_checksum.to_string(),
        desired_media_path: desired_media_path_o.clone(),
        desired_media_extension: ext,
        supp_info: supp_info.clone(),
        modified: scan_info.modified_datetime,
        guessed_datetime,
    };
    Ok(media_file_info)
}

/// `yyyy/mm/dd/hhmm-ssms`
/// OR `undated/checksum`
pub(crate) fn get_desired_media_path(short_checksum: &str, media_datetime: &Option<i64>) -> String {
    let date_dir;
    let name;
    if let Some(dt_ms) = media_datetime {
        let dt = DateTime::from_timestamp_millis(*dt_ms);
        match dt {
            Some(dt) => {
                date_dir = format!("{}/{:0>2}/{:0>2}", dt.year(), dt.month(), dt.day());
                name = format!(
                    "{:0>2}{:0>2}-{:0>2}{:0>3}",
                    dt.hour(),
                    dt.minute(),
                    dt.second(),
                    dt.timestamp_subsec_millis()
                );
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
    format!("{date_dir}/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desired_media_path() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        use crate::util::PsContainer;
        use crate::util::PsDirectoryContainer;
        use crate::util::checksum_bytes;

        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let bytes = c.file_bytes(&"Canon_40D.jpg".to_string()).unwrap();
        let short_checksum = checksum_bytes(&bytes)?.0;

        assert_eq!(
            get_desired_media_path(&short_checksum, &None),
            "undated/6bfdabd".to_string()
        );
        assert_eq!(
            get_desired_media_path(&short_checksum, &Some(1212162961000)),
            "2008/05/30/1556-01000".to_string()
        );
        assert_eq!(
            get_desired_media_path(&short_checksum, &Some(1212162961009)),
            "2008/05/30/1556-01009".to_string()
        );
        Ok(())
    }
}

#[cfg(test)]
impl MediaFileInfo {
    pub(crate) fn new_for_test(
        desired_media_path: Option<String>,
        desired_media_extension: &str,
    ) -> Self {
        MediaFileInfo {
            original_file_this_run: "".to_string(),
            original_path: vec![],
            desired_media_path,
            desired_media_extension: desired_media_extension.to_string(),
            quick_file_type: QuickFileType::Media,
            parsed_exif: None,
            accurate_file_type: AccurateFileType::Jpg,
            short_checksum: "tsc".to_string(),
            long_checksum: "tlc".to_string(),
            supp_info: None,
            modified: None,
            guessed_datetime: None,
        }
    }
}
