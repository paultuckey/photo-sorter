use crate::file_type::AccurateFileType;
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat};
use exif::{Exif, In, Reader, Tag, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;
use log::{debug, warn};

#[derive(Debug, Clone)]
pub(crate) struct ParsedExif {
    pub(crate) datetime_original: Option<String>,
    pub(crate) datetime: Option<String>,
    pub(crate) gps_date: Option<String>,
    pub(crate) unique_id: Option<String>,
}

pub(crate) fn does_file_format_have_exif(file_format: &AccurateFileType) -> bool {
    matches!(file_format, AccurateFileType::Jpg | AccurateFileType::Png | AccurateFileType::Heic)
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
            let unique_id = parse_tag(&exif, Tag::ImageUniqueID);
            let datetime_original = parse_exif_datetime(&parse_tag(&exif, Tag::DateTimeOriginal));
            let datetime = parse_exif_datetime(&parse_tag(&exif, Tag::DateTime));
            let gps_date = parse_exif_date(&parse_tag(&exif, Tag::GPSDateStamp));
            Some(ParsedExif {
                datetime_original,
                datetime,
                gps_date,
                unique_id,
            })
        }
        Err(e) => {
            debug!("Could not read EXIF data from file: {} ({} bytes, err {})", name, bytes.len(), e);
            None
        }
    }
}

pub(crate) fn best_guess_taken_dt(pe: &Option<ParsedExif>) -> Option<String> {
    let Some(pe) = pe else { return None };
    pe.datetime_original
        .clone()
        .or(pe.datetime.clone())
        .or(pe.gps_date.clone())
}

fn all_tags(path: &Path) -> Option<HashMap<String, String>> {
    let file = File::open(path).ok()?;
    let mut buf_reader = BufReader::new(file);
    let exif_reader = Reader::new();
    let exif = exif_reader.read_from_container(&mut buf_reader).ok()?;
    let fields = exif.fields();
    let mut map = HashMap::new();
    for field in fields {
        let tag = field.tag.description()?;
        let value = &field.value;
        //info!("Tag: {:?}, Value: {:?}", tag, value);
        if let Value::Ascii(v) = value {
            if !v.is_empty() {
                let s = String::from_utf8(v[0].to_owned()).ok()?;
                map.insert(tag.to_string(), s);
            }
        }
    }
    Some(map)
}

fn parse_tag(e: &Exif, t: Tag) -> Option<String> {
    let field = e.get_field(t, In::PRIMARY)?;
    if let Value::Ascii(v) = &field.value {
        if !v.is_empty() {
            return String::from_utf8(v[0].to_owned()).ok();
        }
    }
    None
}

/// Sometimes exif dates can be invalid(ish)
///  eg, 2019:04:04 18:04:98 (invalid seconds)
///  eg, 2019:04:04 (missing time)
fn parse_exif_datetime(d: &Option<String>) -> Option<String> {
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
    let ont = NaiveTime::from_hms_opt(hh, mm, ss);
    let Some(nt) = ont else {
        warn!("Could not parse EXIF time: {d:?}");
        return None;
    };
    let ndt = NaiveDateTime::new(nd, nt);
    let utc_dt = ndt.and_utc();
    Some(utc_dt.to_rfc3339_opts(SecondsFormat::Secs, true))
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

    #[tokio::test()]
    async fn test_dt() {
        crate::test_util::setup_log().await;
        let dt = parse_exif_datetime(&Some("2017:08:19 10:21:59".to_string()));
        assert_eq!(dt, Some("2017-08-19T10:21:59Z".to_string()));
        let dt = parse_exif_datetime(&Some("2017:08:19".to_string()));
        assert_eq!(dt, Some("2017-08-19T00:00:00Z".to_string()));
    }

    #[tokio::test()]
    async fn test_dt2() {
        crate::test_util::setup_log().await;
        let dt = parse_exif_datetime(&Some("2019:04:04 18:04:98".to_string()));
        assert_eq!(dt, Some("2019-04-04T18:05:38Z".to_string()));
    }

    #[tokio::test()]
    async fn test_d1() {
        crate::test_util::setup_log().await;
        let d = parse_exif_date(&Some("2019:04:04".to_string()));
        assert_eq!(d, Some("2019-04-04".to_string()));
    }

    #[tokio::test()]
    async fn test_parse_exif_created() {
        use crate::util::PsContainer;
        use crate::util::PsDirectoryContainer;
        let mut c = PsDirectoryContainer::new("test".to_string());
        let bytes = c.file_bytes(&"Canon_40D.jpg".to_string()).unwrap();
        let p = parse_exif(&bytes, &"test".to_string(), &AccurateFileType::Jpg).unwrap();
        assert_eq!(
            p.datetime_original,
            Some("2008-05-30T15:56:01Z".to_string())
        );
    }

    #[tokio::test()]
    async fn test_parse_exif_all_tags() {
        crate::test_util::setup_log().await;
        let p = Path::new("test/Canon_40D.jpg").to_path_buf();
        let t = all_tags(&p).unwrap();
        assert_eq!(t.len(), 10);
        assert_eq!(
            t.get("Interoperability identification"),
            Some(&"R98".to_string())
        );
    }
}