use crate::file_type::AccurateFileType;
use crate::supplemental_info::SupplementalInfo;
use crate::util::ScanInfo;
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat};
use exif::{Exif, Field, In, Reader, Tag, Value};
use log::{debug, warn};
use std::fmt::Display;
use std::io::{BufReader, Cursor};

#[derive(Debug, Clone)]
pub(crate) struct ParsedExif {
    pub(crate) datetime_original: Option<String>,
    pub(crate) datetime: Option<String>,
    pub(crate) gps_date: Option<String>,
    pub(crate) unique_id: Option<String>,
}

fn exif_dt_as_epoch_ms(dt: String) -> Option<i64> {
    let dt = NaiveDateTime::parse_from_str(&dt, "%Y-%m-%dT%H:%M:%S%.fZ").ok()?;
    Some(dt.and_utc().timestamp_millis())
}

fn exif_d_as_epoch_ms(dt: String) -> Option<i64> {
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
    let exif_reader = Reader::new();
    let mut cursor = Cursor::new(bytes);
    let mut bufread_seek = BufReader::new(&mut cursor);
    let exif_r = exif_reader.read_from_container(&mut bufread_seek);
    match exif_r {
        Ok(exif) => {
            let unique_id = parse_ascii_tag(&exif, Tag::ImageUniqueID);
            let datetime_original =
                parse_exif_datetime_with_ms(&exif, Tag::DateTimeOriginal, Tag::SubSecTimeOriginal);
            let datetime = parse_exif_datetime_with_ms(&exif, Tag::DateTime, Tag::SubSecTime);
            let gps_date = parse_exif_date(&parse_ascii_tag(&exif, Tag::GPSDateStamp));
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
    si: &ScanInfo,
    supp_info: &Option<SupplementalInfo>,
) -> Option<i64> {
    if let Some(dt) = supp_info
        .as_ref()
        .and_then(|si| si.photo_taken_time.as_ref())
        .and_then(|si_dt| si_dt.timestamp_as_epoch_ms())
    {
        return Some(dt);
    }
    if let Some(dt) = pe_o
        .as_ref()
        .and_then(|pe| pe.datetime_original.clone())
        .and_then(exif_dt_as_epoch_ms)
    {
        return Some(dt);
    }
    if let Some(dt) = pe_o
        .as_ref()
        .and_then(|pe| pe.datetime.clone())
        .and_then(exif_dt_as_epoch_ms)
    {
        return Some(dt);
    }
    if let Some(dt) = pe_o
        .as_ref()
        .and_then(|pe| pe.gps_date.clone())
        .and_then(exif_d_as_epoch_ms)
    {
        return Some(dt);
    }
    if let Some(dt) = supp_info
        .as_ref()
        .and_then(|si| si.creation_time.as_ref())
        .and_then(|si_dt| si_dt.timestamp_as_epoch_ms())
    {
        return Some(dt);
    }
    if let Some(dt) = si.modified_datetime {
        return Some(dt);
    }
    if let Some(dt) = si.created_datetime {
        return Some(dt);
    }
    None
}

fn parse_ascii_tag(e: &Exif, t: Tag) -> Option<String> {
    let field = e.get_field(t, In::PRIMARY)?;
    if let Value::Ascii(v) = &field.value
        && !v.is_empty()
    {
        return String::from_utf8(v[0].to_owned()).ok();
    }
    None
}

pub(crate) struct ExifTag {
    pub(crate) tag_code: String,
    pub(crate) tag_desc: Option<String>,
    pub(crate) tag_value: Option<String>,
    pub(crate) tag_type: Option<String>,
}

pub(crate) fn all_tags(bytes: &Vec<u8>) -> Vec<ExifTag> {
    let mut tags = vec![];
    let exif_reader = Reader::new();
    let mut cursor = Cursor::new(bytes);
    let mut bufread_seek = BufReader::new(&mut cursor);
    let exif_r = exif_reader.read_from_container(&mut bufread_seek);
    match exif_r {
        Ok(exif) => {
            let fields = exif.fields();
            for field in fields {
                let s_o = field_to_opt_string(field);

                tags.push(ExifTag {
                    tag_code: field.tag.to_string(),
                    tag_desc: field.tag.description().map(|s| s.to_string()),
                    tag_value: s_o,
                    tag_type: match &field.value {
                        Value::Byte(_) => Some("Byte".to_string()),
                        Value::Ascii(_) => Some("Ascii".to_string()),
                        Value::Short(_) => Some("Short".to_string()),
                        Value::Long(_) => Some("Long".to_string()),
                        Value::Rational(_) => Some("Rational".to_string()),
                        Value::SByte(_) => Some("SByte".to_string()),
                        Value::Undefined(_, _) => Some("Undefined".to_string()),
                        Value::SShort(_) => Some("SShort".to_string()),
                        Value::SLong(_) => Some("SLong".to_string()),
                        Value::SRational(_) => Some("SRational".to_string()),
                        Value::Float(_) => Some("Float".to_string()),
                        Value::Double(_) => Some("Double".to_string()),
                        Value::Unknown(_, _, _) => Some("Unknown".to_string()),
                    },
                });
            }
        }
        Err(e) => {
            warn!("Could not read EXIF data: {e}");
        }
    }
    tags
}

fn field_to_opt_string(field: &Field) -> Option<String> {
    match &field.value {
        Value::Byte(v) => {
            return vec_to_string(v);
        }
        Value::Ascii(v) => {
            if !v.is_empty() {
                // return all strings from v: Vec<Vec<u8>> concatenated with a comma
                let s = v
                    .iter()
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .collect::<Vec<String>>()
                    .join(", ");
                return Some(s);
            }
        }
        Value::Short(v) => {
            return vec_to_string(v);
        }
        Value::Long(v) => {
            return vec_to_string(v);
        }
        Value::Rational(v) => {
            return vec_to_string(v);
        }
        Value::SByte(v) => {
            return vec_to_string(v);
        }
        Value::Undefined(_, _) => {
            return Some("<Undefined>".to_string());
        }
        Value::SShort(v) => {
            return vec_to_string(v);
        }
        Value::SLong(v) => {
            return vec_to_string(v);
        }
        Value::SRational(v) => {
            return vec_to_string(v);
        }
        Value::Float(v) => {
            return vec_to_string(v);
        }
        Value::Double(v) => {
            return vec_to_string(v);
        }
        Value::Unknown(_, _, _) => {
            return Some("<Unknown>".to_string());
        }
    }
    None
}

fn vec_to_string<T: Display>(v: &[T]) -> Option<String> {
    if !v.is_empty() {
        let s = v
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        return Some(s);
    }
    None
}

fn parse_exif_datetime_with_ms(exif: &Exif, dt_tag: Tag, sub_second_tag: Tag) -> Option<String> {
    let dt_tag = &parse_ascii_tag(exif, dt_tag);
    let mut ms = None;
    let ss_o = &parse_ascii_tag(exif, sub_second_tag);
    if let Some(ss) = ss_o {
        let ms_r = ss.parse::<u32>();
        if let Ok(ms2) = ms_r {
            ms = Some(ms2)
        }
    }
    parse_exif_datetime(dt_tag, ms)
}

/// Sometimes exif dates can be invalid(ish)
///  eg, 2019:04:04 18:04:98 (invalid seconds)
///  eg, 2019:04:04 (missing time)
fn parse_exif_datetime(d: &Option<String>, ms_o: Option<u32>) -> Option<String> {
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
    if let Some(ms) = ms_o {
        if ms < 1000 {
            ont = NaiveTime::from_hms_milli_opt(hh, mm, ss, ms);
        } else {
            warn!("MS invalid for: {d:?} + {ms}");
        }
    }
    let Some(nt) = ont else {
        warn!("Could not parse EXIF time: {d:?}");
        return None;
    };
    let ndt = NaiveDateTime::new(nd, nt);
    let utc_dt = ndt.and_utc();
    if ms_o.is_some() {
        Some(utc_dt.to_rfc3339_opts(SecondsFormat::Millis, true))
    } else {
        Some(utc_dt.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
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
        let dt = parse_exif_datetime(&Some("2017:08:19 10:21:59".to_string()), Some(123));
        assert_eq!(dt, Some("2017-08-19T10:21:59.123Z".to_string()));
        let dt = parse_exif_datetime(&Some("2017:08:19".to_string()), None);
        assert_eq!(dt, Some("2017-08-19T00:00:00Z".to_string()));
    }

    #[test]
    fn test_dt2() {
        crate::test_util::setup_log();
        let dt = parse_exif_datetime(&Some("2019:04:04 18:04:98".to_string()), Some(2000));
        assert_eq!(dt, Some("2019-04-04T18:05:38.000Z".to_string()));
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
            exif_dt_as_epoch_ms("2008-05-30T15:56:01Z".to_string()),
            Some(1212162961000)
        );
        assert_eq!(
            exif_d_as_epoch_ms("2008-05-30".to_string()),
            Some(1212105600000)
        );
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
                "DateTime",
                "YCbCrPositioning",
                "ExposureTime",
                "FNumber",
                "ExposureProgram",
                "PhotographicSensitivity",
                "ExifVersion",
                "DateTimeOriginal",
                "DateTimeDigitized",
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
                "FlashpixVersion",
                "ColorSpace",
                "PixelXDimension",
                "PixelYDimension",
                "InteroperabilityIndex",
                "InteroperabilityVersion",
                "FocalPlaneXResolution",
                "FocalPlaneYResolution",
                "FocalPlaneResolutionUnit",
                "CustomRendered",
                "ExposureMode",
                "WhiteBalance",
                "SceneCaptureType",
                "GPSVersionID",
                "Compression",
                "XResolution",
                "YResolution",
                "ResolutionUnit",
                "JPEGInterchangeFormat",
                "JPEGInterchangeFormatLength"
            ]
        );

        let first_tag = t.first().unwrap();
        assert_eq!(first_tag.tag_code, "Make".to_string());
        assert_eq!(
            first_tag.tag_desc,
            Some("Manufacturer of image input equipment".to_string())
        );
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
