use crate::file_type::AccurateFileType;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat};
use log::{debug, warn};
use nom_exif::{EntryValue, Exif, ExifIter, MediaParser, MediaSource, ParsedExifEntry};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::io::Cursor;

#[derive(Debug, Clone)]
pub(crate) struct ParsedExif {
    pub(crate) datetime_original: Option<String>,
    pub(crate) datetime: Option<String>,
    pub(crate) gps_date: Option<String>,
    pub(crate) unique_id: Option<String>,
}

pub(crate) fn dt_as_epoch_ms(dt: String) -> Option<i64> {
    let dt = DateTime::parse_from_rfc3339(&dt).ok()?;
    Some(dt.timestamp_millis())
}

pub(crate) fn d_as_epoch_ms(dt: String) -> Option<i64> {
    let d = NaiveDate::parse_from_str(&dt, "%Y-%m-%d").ok()?;
    let dt = d.and_hms_milli_opt(0, 0, 0, 0)?;
    Some(dt.and_utc().timestamp_millis())
}

pub(crate) fn does_file_format_have_exif(file_format: &AccurateFileType) -> bool {
    matches!(
        file_format,
        AccurateFileType::Jpg | AccurateFileType::Png | AccurateFileType::Heic
    )
}

pub(crate) fn parse_exif(
    bytes: &Vec<u8>,
    name: &String,
    file_format: &AccurateFileType,
) -> Option<ParsedExif> {
    if !does_file_format_have_exif(file_format) {
        return None;
    }
    let mut parser = MediaParser::new();
    let ms = MediaSource::seekable(Cursor::new(bytes));
    let Ok(ms) = ms else {
        debug!(
            "Could not create MediaSource for file: {} ({} bytes)",
            name,
            bytes.len()
        );
        return None;
    };
    if !ms.has_exif() {
        debug!("File does not mave exif metadata: {}", name);
        return None;
    }
    let exif_iter_r: nom_exif::Result<ExifIter> = parser.parse(ms);

    match exif_iter_r {
        Ok(exif_iter) => {
            let exif: Exif = exif_iter.into();
            let unique_id = parse_ascii_tag(&exif.get(nom_exif::ExifTag::ImageUniqueID));
            let datetime_original = parse_exif_datetime_with_ms(
                &exif,
                nom_exif::ExifTag::DateTimeOriginal, // 0x9003
                nom_exif::ExifTag::SubSecTimeOriginal,
            );
            let datetime = parse_exif_datetime_with_ms(
                &exif,
                nom_exif::ExifTag::ModifyDate, // DateTime (0x0132)
                nom_exif::ExifTag::SubSecTime,
            );
            let gps_date =
                parse_exif_date(&parse_ascii_tag(&exif.get(nom_exif::ExifTag::GPSDateStamp)));
            Some(ParsedExif {
                datetime_original,
                datetime,
                gps_date,
                unique_id,
            })
        }
        Err(e) => {
            debug!(
                "Could not read EXIF data from file: {} ({} bytes, err {})",
                name,
                bytes.len(),
                e
            );
            None
        }
    }
}

fn parse_ascii_tag(e: &Option<&EntryValue>) -> Option<String> {
    let Some(e) = e else {
        return None;
    };
    Some(e.to_string())
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct ExifInfo {
    pub(crate) tags: Vec<ExifTag>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct ExifTag {
    pub(crate) tag_code: String,
    pub(crate) tag_value: Option<String>,
}

pub(crate) fn exif_info(bytes: &Vec<u8>) -> ExifInfo {
    ExifInfo {
        tags: all_tags(bytes),
    }
}

pub(crate) fn all_tags(bytes: &Vec<u8>) -> Vec<ExifTag> {
    let mut tags = vec![];
    let mut parser = MediaParser::new();
    let ms = MediaSource::seekable(Cursor::new(bytes));
    let Ok(ms) = ms else {
        debug!("Could not create MediaSource");
        return vec![];
    };
    if !ms.has_exif() {
        debug!("File does not mave exif metadata");
        return vec![];
    }
    let exif_iter_r: nom_exif::Result<ExifIter> = parser.parse(ms);
    match exif_iter_r {
        Ok(exif_iter) => {
            for entry in exif_iter {
                let s_o = field_to_opt_string(&entry);
                let Some(t) = entry.tag() else {
                    continue; // only support recognised tags
                };
                tags.push(ExifTag {
                    tag_code: t.to_string(),
                    tag_value: s_o,
                });
            }
        }
        Err(e) => {
            warn!("Could not read EXIF data: {e}");
        }
    }
    tags
}

fn field_to_opt_string(field: &ParsedExifEntry) -> Option<String> {
    if let Ok(value) = field.clone().take_result() {
        return Some(value.to_string());
    }
    None
}

fn parse_exif_datetime_with_ms(
    exif: &Exif,
    dt_tag: nom_exif::ExifTag,
    sub_second_tag: nom_exif::ExifTag,
) -> Option<String> {
    let dt_tag = &parse_ascii_tag(&exif.get(dt_tag));
    let mut sub_second_val = None;
    let sub_second_o = &parse_ascii_tag(&exif.get(sub_second_tag));
    if let Some(sub_second_s) = sub_second_o {
        let sub_second_u_r = sub_second_s.parse::<u32>();
        if let Ok(sub_second_u) = sub_second_u_r {
            sub_second_val = Some(sub_second_u)
        }
    }
    parse_exif_datetime(dt_tag, sub_second_val)
}

/// Sometimes exif dates can be invalid(ish)
///  eg, 2019:04:04 18:04:98 (invalid seconds)
///  eg, 2019:04:04 (missing time)
fn parse_exif_datetime(d: &Option<String>, sub_second_o: Option<u32>) -> Option<String> {
    let Some(d) = d else { return None };
    // 2017:08:19 10:21:59
    let s = d.split(' ').collect::<Vec<&str>>();
    let s1 = s.first()?;
    let nd_result = NaiveDate::parse_from_str(s1, "%Y-%m-%d");
    let nd = match nd_result {
        Ok(nd) => nd,
        Err(e) => {
            warn!("Could not parsing EXIF date: {d:?} {e}");
            return None;
        }
    };
    let s2_r = s.get(1);
    let Some(s2) = s2_r else {
        // no time
        let ndt = NaiveDateTime::new(nd, NaiveTime::default());
        let utc_dt = ndt.and_utc();
        return Some(utc_dt.to_rfc3339_opts(SecondsFormat::Secs, true));
    };
    let s2_parts = s2.split(':').collect::<Vec<&str>>();
    let hh = s2_parts.first()?.parse::<u32>().ok()?;
    let mut mm = s2_parts.get(1)?.parse::<u32>().ok()?;
    let mut ss = s2_parts.get(2)?.parse::<u32>().ok()?;
    while ss > 60 {
        ss -= 60;
        mm += 1;
    }
    let mut ont = NaiveTime::from_hms_opt(hh, mm, ss);
    if let Some(sub_second) = sub_second_o {
        let sub_second_ms = sub_second_time_to_ms(sub_second);
        ont = NaiveTime::from_hms_milli_opt(hh, mm, ss, sub_second_ms);
    }
    let Some(nt) = ont else {
        warn!("Could not parse EXIF time: {d:?}");
        return None;
    };
    let ndt = NaiveDateTime::new(nd, nt);
    let utc_dt = ndt.and_utc();
    if sub_second_o.is_some() {
        Some(utc_dt.to_rfc3339_opts(SecondsFormat::Millis, true))
    } else {
        Some(utc_dt.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

/// SubSecTime = 2 means .2 seconds (200ms).
/// SubSecTime = 23 means .23 seconds (230ms).
/// SubSecTime = 234 means .234 seconds (234ms).
/// SubSecTime = 2345 means .2345 seconds.
fn sub_second_time_to_ms(sub_second_val: u32) -> u32 {
    if sub_second_val == 0 {
        return 0;
    }
    let digits = (sub_second_val as f64).log10().floor() as u32 + 1;
    let scale = 10f64.powi(digits as i32);
    let ms = (sub_second_val as f64 / scale) * 1000.0;
    ms.round() as u32
}

fn parse_exif_date(d: &Option<String>) -> Option<String> {
    let Some(d) = d else { return None };
    // 2017:08:19 10:21:59
    let s = d.split(' ').collect::<Vec<&str>>();
    let s1 = s.first()?;
    let nd_result = NaiveDate::parse_from_str(s1, "%Y:%m:%d");
    let nd = match nd_result {
        Ok(nd) => nd,
        Err(e) => {
            warn!("Could not parsing EXIF date: {d:?} {e}");
            return None;
        }
    };
    Some(format!(
        "{:0>2}-{:0>2}-{:0>2}",
        nd.year(),
        nd.month(),
        nd.day()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::PsContainer;
    use crate::util::PsDirectoryContainer;

    #[test]
    fn test_dt() {
        crate::test_util::setup_log();
        let dt = parse_exif_datetime(&Some("2017-08-19 10:21:59".to_string()), Some(123));
        assert_eq!(dt, Some("2017-08-19T10:21:59.123Z".to_string()));
        let dt = parse_exif_datetime(&Some("2017-08-19".to_string()), None);
        assert_eq!(dt, Some("2017-08-19T00:00:00Z".to_string()));
    }

    #[test]
    fn test_dt2() {
        crate::test_util::setup_log();
        let dt = parse_exif_datetime(&Some("2019-04-04 18:04:98".to_string()), Some(2000));
        assert_eq!(dt, Some("2019-04-04T18:05:38.200Z".to_string()));
    }

    #[test]
    fn test_d1() {
        crate::test_util::setup_log();
        let d = parse_exif_date(&Some("2019:04:04".to_string()));
        assert_eq!(d, Some("2019-04-04".to_string()));
    }

    #[test]
    fn test_parse_exif_dt() {
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let bytes = c.file_bytes(&"Canon_40D.jpg".to_string()).unwrap();
        let p = parse_exif(&bytes, &"test".to_string(), &AccurateFileType::Jpg).unwrap();
        assert_eq!(
            p.datetime_original,
            Some("2008-05-30T15:56:01.000Z".to_string())
        );
    }

    #[test]
    fn test_exif_date_epoch_ms() {
        assert_eq!(
            dt_as_epoch_ms("2008-05-30T15:56:01Z".to_string()),
            Some(1212162961000)
        );
        assert_eq!(d_as_epoch_ms("2008-05-30".to_string()), Some(1212105600000));
    }

    #[test]
    fn test_subsectime_to_ms() {
        assert_eq!(sub_second_time_to_ms(2), 200);
        assert_eq!(sub_second_time_to_ms(23), 230);
        assert_eq!(sub_second_time_to_ms(234), 234);
        assert_eq!(sub_second_time_to_ms(2345), 235);
    }

    #[test]
    fn test_parse_exif_mp4() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let bytes = c.file_bytes(&"Hello.mp4".to_string())?;
        let t = all_tags(&bytes);
        assert_eq!(t.len(), 0);
        Ok(())
    }

    #[test]
    fn test_parse_exif_all_tags() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let bytes = c.file_bytes(&"Canon_40D.jpg".to_string())?;
        let t = all_tags(&bytes);
        assert_eq!(t.len(), 47);

        let tag_names: Vec<String> = t.iter().map(|tag| tag.tag_code.clone()).collect();
        assert_eq!(
            tag_names,
            vec![
                "Make",
                "Model",
                "Orientation",
                "XResolution",
                "YResolution",
                "ResolutionUnit",
                "Software",
                "ModifyDate",
                "YCbCrPositioning",
                "ExifOffset",
                "ExposureTime",
                "FNumber",
                "ExposureProgram",
                "ISOSpeedRatings",
                "ExifVersion",
                "DateTimeOriginal",
                "CreateDate",
                "ComponentsConfiguration",
                "ShutterSpeedValue",
                "ApertureValue",
                "ExposureBiasValue",
                "MeteringMode",
                "Flash",
                "FocalLength",
                "UserComment",
                "SubSecTime",
                "SubSecTimeOriginal",
                "SubSecTimeDigitized",
                "FlashPixVersion",
                "ColorSpace",
                "ExifImageWidth",
                "ExifImageHeight",
                "InteropOffset",
                "FocalPlaneXResolution",
                "FocalPlaneYResolution",
                "FocalPlaneResolutionUnit",
                "CustomRendered",
                "ExposureMode",
                "WhiteBalanceMode",
                "SceneCaptureType",
                "GPSInfo",
                "Compression",
                "XResolution",
                "YResolution",
                "ResolutionUnit",
                "ThumbnailOffset",
                "ThumbnailLength"
            ]
        );

        let first_tag = t.first().unwrap();
        assert_eq!(first_tag.tag_code, "Make".to_string());
        assert_eq!(first_tag.tag_value, Some("Canon".to_string()));

        // SubSecTimeOriginal
        let sub_sec_time_original = t
            .iter()
            .find(|tag| tag.tag_code == "SubSecTimeOriginal")
            .unwrap();
        assert_eq!(
            sub_sec_time_original.tag_value.clone().unwrap(),
            "00".to_string()
        );
        Ok(())
    }
}
