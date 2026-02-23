use crate::db_cmd::HashInfo;
use crate::exif_util::{PsExifInfo, best_guess_taken_exif, parse_exif_info};
use crate::file_type::{
    AccurateFileType, MetadataType, QuickFileType, determine_file_type, file_ext_from_file_type,
    metadata_type,
};
use crate::fs::FileSystem;
use crate::supplemental_info::PsSupplementalInfo;
use crate::track_util::{PsTrackInfo, parse_track_info};
use crate::util::ScanInfo;
use anyhow::anyhow;
use chrono::{DateTime, Datelike, Timelike};
use serde::{Deserialize, Serialize};
use std::io::Seek;
use tracing::warn;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct MediaFileInfo {
    pub(crate) original_file_this_run: String,
    pub(crate) original_path: Vec<String>,
    pub(crate) quick_file_type: QuickFileType,
    pub(crate) exif_info: Option<PsExifInfo>,
    pub(crate) track_info: Option<PsTrackInfo>,
    pub(crate) accurate_file_type: AccurateFileType,
    pub(crate) hash_info: HashInfo,
    pub(crate) supp_info: Option<PsSupplementalInfo>,
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
    si: &ScanInfo,
    root: &dyn FileSystem,
    supp_info: &Option<PsSupplementalInfo>,
    hash_info: &HashInfo,
) -> anyhow::Result<MediaFileInfo> {
    let name = &si.file_path;
    let mut reader = root.open(&si.file_path.to_string())?;
    let guessed_ff = determine_file_type(&mut reader, name);
    if guessed_ff == AccurateFileType::Unsupported {
        warn!("Not a valid media file {name:?}");
        return Err(anyhow!("File is not a valid media file"));
    }
    reader.seek(std::io::SeekFrom::Start(0))?;

    let mut exif_o = None;
    let mut track_o = None;
    match metadata_type(&guessed_ff) {
        MetadataType::ExifTags => {
            exif_o = parse_exif_info(&mut reader);
        }
        MetadataType::Track => {
            track_o = parse_track_info(&mut reader);
        }
        MetadataType::NoMetadata => {}
    }
    let hash_info = hash_info.clone();

    let media_file_info = MediaFileInfo {
        original_file_this_run: name.clone(),
        original_path: vec![name.clone()],
        accurate_file_type: guessed_ff.clone(),
        quick_file_type: si.quick_file_type.clone(),
        exif_info: exif_o.clone(),
        track_info: track_o.clone(),
        hash_info,
        supp_info: supp_info.clone(),
        modified: si.modified_datetime,
        created: si.created_datetime,
    };
    Ok(media_file_info)
}

pub(crate) fn media_file_derived_from_media_info(
    media_info: &MediaFileInfo,
) -> anyhow::Result<MediaFileDerivedInfo> {
    let ext = file_ext_from_file_type(&media_info.accurate_file_type);
    let guessed_datetime = best_guess_taken_dt(media_info);
    let short_checksum = &media_info.hash_info.short_checksum;
    let desired_media_path_o = Some(get_desired_media_path(short_checksum, &guessed_datetime));
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
///
/// Result returned as ISO 8601 string
pub(crate) fn best_guess_taken_dt(info: &MediaFileInfo) -> Option<String> {
    if let Some(dt) = info
        .supp_info
        .as_ref()
        .and_then(|si| si.photo_taken_time.as_ref())
        .and_then(|si_dt| si_dt.timestamp_s_as_iso_8601())
    {
        return Some(dt);
    }
    let time_taken_from_exif = best_guess_taken_exif(&info.exif_info);
    if let Some(dt) = time_taken_from_exif {
        return Some(dt);
    }
    if let Some(dt) = info
        .supp_info
        .as_ref()
        .and_then(|si| si.creation_time.as_ref())
        .and_then(|si_dt| si_dt.timestamp_s_as_iso_8601())
    {
        return Some(dt);
    }
    if let Some(dt) = info.created {
        let o = crate::util::timestamp_to_rfc3339(dt);
        if let Some(dt) = o {
            return Some(dt);
        }
    }
    if let Some(dt) = info.modified {
        let o = crate::util::timestamp_to_rfc3339(dt);
        if let Some(dt) = o {
            return Some(dt);
        }
    }
    None
}

/// `yyyy/mm/dd/hhmm-ssms`
/// OR `undated/checksum`
pub(crate) fn get_desired_media_path(
    short_checksum: &str,
    media_datetime: &Option<String>,
) -> String {
    let date_dir;
    let name;
    if let Some(dt_s) = media_datetime {
        let dt_r = DateTime::parse_from_rfc3339(dt_s);
        match dt_r {
            Ok(dt) => {
                date_dir = format!("{}/{:0>2}/{:0>2}", dt.year(), dt.month(), dt.day());
                name = format!(
                    "{:0>2}{:0>2}-{:0>2}{:0>3}",
                    dt.hour(),
                    dt.minute(),
                    dt.second(),
                    dt.timestamp_subsec_millis()
                );
            }
            Err(_) => {
                warn!("Could not parse datetime: {dt_s:?}");
                date_dir = "undated".to_string();
                name = short_checksum.to_string();
            }
        }
    } else {
        date_dir = "undated".to_string();
        name = short_checksum.to_string();
    }
    format!("{date_dir}/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::OsFileSystem;

    #[test]
    fn test_best_guess_taken_dt_timestamps() {
        let mut info = MediaFileInfo::new_for_test();
        // 1000000000000 ms = 2001-09-09T01:46:40Z
        let ts = 1000000000000;

        // Test created timestamp
        info.created = Some(ts);
        info.modified = None;
        let dt = best_guess_taken_dt(&info).expect("Should have a date from created");
        assert_eq!(dt, "2001-09-09T01:46:40+00:00");

        // Test modified timestamp
        info.created = None;
        info.modified = Some(ts);
        let dt = best_guess_taken_dt(&info).expect("Should have a date from modified");
        assert_eq!(dt, "2001-09-09T01:46:40+00:00");
    }

    #[test]
    fn test_desired_media_path() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        use crate::util::checksum_bytes;

        let c = OsFileSystem::new(&"test".to_string());
        let reader = c.open(&"Canon_40D.jpg".to_string()).unwrap();
        let short_checksum = checksum_bytes(reader)?.short_checksum;

        assert_eq!(
            get_desired_media_path(&short_checksum, &None),
            "undated/6bfdabd".to_string()
        );
        assert_eq!(
            get_desired_media_path(&short_checksum, &Some("2008-05-30T15:56:01Z".to_string())),
            "2008/05/30/1556-01000".to_string()
        );
        assert_eq!(
            get_desired_media_path(
                &short_checksum,
                &Some("2008-05-30T15:56:01.009Z".to_string())
            ),
            "2008/05/30/1556-01009".to_string()
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_perf_benchmark_zip_read() {
        crate::test_util::setup_log();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        // Ensure test file exists
        let zip_path = "test/Canon_40D.jpg.zip";
        let fs = crate::fs::ZipFileSystem::new(zip_path, tz).expect("Failed to open zip");

        let file_path = "Canon_40D.jpg";
        let si = ScanInfo::new(file_path.to_string(), None, None);
        let hash_info = HashInfo { short_checksum: "dummy".to_string(), long_checksum: "dummy".to_string() };

        let start = std::time::Instant::now();
        for _ in 0..100 {
            let _ = media_file_info_from_readable(&si, &fs, &None, &hash_info);
        }
        let duration = start.elapsed();
        println!("Time taken for 100 iterations: {:?}", duration);
    }
}

#[cfg(test)]
impl MediaFileInfo {
    pub(crate) fn new_for_test() -> Self {
        MediaFileInfo {
            original_file_this_run: "".to_string(),
            original_path: vec![],
            quick_file_type: QuickFileType::Media,
            exif_info: None,
            track_info: None,
            accurate_file_type: AccurateFileType::Jpg,
            hash_info: HashInfo {
                short_checksum: "tsc".to_string(),
                long_checksum: "tlc".to_string(),
            },
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
