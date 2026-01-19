use crate::exif::{ParsedExif, d_as_epoch_ms, dt_as_epoch_ms, parse_exif};
use crate::file_type::{
    AccurateFileType, MetadataType, QuickFileType, determine_file_type, file_ext_from_file_type,
    metadata_type,
};
use crate::mp4_util;
use crate::mp4_util::ParsedMp4;
use crate::supplemental_info::SupplementalInfo;
use crate::util::ScanInfo;
use anyhow::anyhow;
use chrono::{DateTime, Datelike, Timelike};
use log::warn;

#[derive(Debug, Clone)]
pub(crate) struct MediaFileInfo {
    pub(crate) original_file_this_run: String,
    pub(crate) original_path: Vec<String>,
    pub(crate) quick_file_type: QuickFileType,
    pub(crate) parsed_exif: Option<ParsedExif>,
    pub(crate) parsed_mp4: Option<ParsedMp4>,
    pub(crate) accurate_file_type: AccurateFileType,
    pub(crate) short_checksum: String,
    pub(crate) long_checksum: String,
    pub(crate) supp_info: Option<SupplementalInfo>,
    // Modified time of the file
    pub(crate) modified: Option<i64>,
    pub(crate) created: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct MediaFileDerivedInfo {
    /// Desired path relative to output directory, minus the dot and file extension (eg, 2025/09/10/1234-56-789)
    pub(crate) desired_media_path: Option<String>,
    /// Desired file extension (eg, jpg, mp4)
    pub(crate) desired_media_extension: String,
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

    let mut exif_o = None;
    let mut mp4_o = None;
    match metadata_type(&guessed_ff) {
        MetadataType::Exif => {
            exif_o = parse_exif(bytes, name, &guessed_ff);
        }
        MetadataType::Mp4 => {
            mp4_o = mp4_util::extract_mp4_metadata(bytes).ok();
        }
        MetadataType::NoMetadata => {}
    }
    let media_file_info = MediaFileInfo {
        original_file_this_run: name.clone(),
        original_path: vec![name.clone()],
        accurate_file_type: guessed_ff.clone(),
        quick_file_type: scan_info.quick_file_type.clone(),
        parsed_exif: exif_o.clone(),
        parsed_mp4: mp4_o.clone(),
        short_checksum: short_checksum.to_string(),
        long_checksum: long_checksum.to_string(),
        supp_info: supp_info.clone(),
        modified: scan_info.modified_datetime,
        created: scan_info.created_datetime,
    };
    Ok(media_file_info)
}

pub(crate) fn media_file_derived_from_media_info(
    media_info: &MediaFileInfo,
) -> anyhow::Result<MediaFileDerivedInfo> {
    let ext = file_ext_from_file_type(&media_info.accurate_file_type);
    let guessed_datetime = best_guess_taken_dt(
        &media_info.parsed_exif,
        &media_info.supp_info,
        media_info.modified,
        media_info.created,
    );
    let desired_media_path_o = Some(get_desired_media_path(
        &media_info.short_checksum,
        &guessed_datetime,
    ));
    let media_file_info = MediaFileDerivedInfo {
        desired_media_path: desired_media_path_o.clone(),
        desired_media_extension: ext,
    };
    Ok(media_file_info)
}

/// Best guess at the date the photo was taken from messy optional data, in the order of preference:
/// 1. SupplementalInfo photo_taken_time
/// 2. EXIF DateTimeOriginal
/// 3. EXIF DateTime
/// 4. EXIF GPSDateStamp - only accurate up to minute
/// 5. SupplementalInfo creation_time
/// 6. File modified time
///   - no timezone info, unreliable in zips, somewhat unreliable in directories due to file
///     copying / syncing not preserving, only use as second to last resort
/// 7. File creation time
///   - no timezone info, unavailable in zips, somewhat unreliable in directories due to file
///     copying / syncing not preserving, only use as a last resort
pub(crate) fn best_guess_taken_dt(
    pe_o: &Option<ParsedExif>,
    supp_info: &Option<SupplementalInfo>,
    modified_datetime: Option<i64>,
    created_datetime: Option<i64>,
) -> Option<i64> {
    if let Some(dt) = supp_info
        .as_ref()
        .and_then(|si| si.photo_taken_time.as_ref())
        .and_then(|si_dt| si_dt.timestamp_as_epoch_ms())
    {
        if dt.to_string().len() <= 11 {
            warn!("File modified datetime {:?}", dt);
        }
        return Some(dt);
    }
    if let Some(dt) = pe_o
        .as_ref()
        .and_then(|pe| pe.datetime_original.clone())
        .and_then(dt_as_epoch_ms)
    {
        if dt.to_string().len() <= 11 {
            warn!("File modified datetime {:?}", dt);
        }
        return Some(dt);
    }
    if let Some(dt) = pe_o
        .as_ref()
        .and_then(|pe| pe.datetime.clone())
        .and_then(dt_as_epoch_ms)
    {
        if dt.to_string().len() <= 11 {
            warn!("File modified datetime {:?}", dt);
        }
        return Some(dt);
    }
    if let Some(dt) = pe_o
        .as_ref()
        .and_then(|pe| pe.gps_date.clone())
        .and_then(d_as_epoch_ms)
    {
        if dt.to_string().len() <= 11 {
            warn!("File modified datetime {:?}", dt);
        }
        return Some(dt);
    }
    if let Some(dt) = supp_info
        .as_ref()
        .and_then(|si| si.creation_time.as_ref())
        .and_then(|si_dt| si_dt.timestamp_as_epoch_ms())
    {
        if dt.to_string().len() <= 11 {
            warn!("File modified datetime {:?}", dt);
        }
        return Some(dt);
    }
    if let Some(dt) = modified_datetime {
        return Some(dt);
    }
    if let Some(dt) = created_datetime {
        return Some(dt);
    }
    None
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
    pub(crate) fn new_for_test() -> Self {
        MediaFileInfo {
            original_file_this_run: "".to_string(),
            original_path: vec![],
            quick_file_type: QuickFileType::Media,
            parsed_exif: None,
            parsed_mp4: None,
            accurate_file_type: AccurateFileType::Jpg,
            short_checksum: "tsc".to_string(),
            long_checksum: "tlc".to_string(),
            supp_info: None,
            modified: None,
            created: None,
        }
    }
}

#[cfg(test)]
impl MediaFileDerivedInfo {
    pub(crate) fn new_for_test(
        desired_media_path: Option<String>,
        desired_media_extension: &str,
    ) -> Self {
        MediaFileDerivedInfo {
            desired_media_path,
            desired_media_extension: desired_media_extension.to_string(),
        }
    }
}
