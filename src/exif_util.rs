use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, Timelike};
use exif::{Exif, In, Reader, Tag, Value};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;
use tracing::{debug, warn};
use crate::media_file::{MediaFileReadable};

#[derive(Debug, Clone)]
pub(crate) struct ParsedExif {
    pub(crate) datetime_original: Option<String>,
    pub(crate) datetime: Option<String>,
    pub(crate) gps_date: Option<String>,
    pub(crate) unique_id: Option<String>,
}

pub(crate) fn parse_exif(media_file_reader: &dyn MediaFileReadable) -> Option<ParsedExif> {
    let bytes_res = media_file_reader.to_bytes();
    let Ok(file_reader) = bytes_res else {
        warn!("Could not read file: {}", media_file_reader.name());
        return None;
    };
    let mut buffer = BufReader::new(Cursor::new(file_reader));
    let exif_reader = Reader::new();
    let exif_r = exif_reader.read_from_container(&mut buffer);
    let Ok(exif) = exif_r else {
        warn!("Could not read EXIF data from file: {}", media_file_reader.name());
        return None;
    };
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

pub(crate) fn best_guess_taken_dt(pe: &Option<ParsedExif>) -> Option<String> {
    let Some(pe) = pe else { return None };
    pe.datetime_original.clone()
        .or(pe.datetime.clone())
        .or(pe.gps_date.clone())
}

pub(crate) fn all_tags(path: &Path) -> Option<HashMap<String, String>> {
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
            warn!("Could not parsing EXIF date: {:?} {}", d, e);
            return None;
        }
    };
    let s2_r = s.get(1);
    let Some(s2) = s2_r else { // no time
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
        warn!("Could not parse EXIF time: {:?}", d);
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
            warn!("Could not parsing EXIF date: {:?} {}", d, e);
            return None;
        }
    };
    Some(format!("{:0>2}-{:0>2}-{:0>2}", nd.year(), nd.month(), nd.day()))
}

pub(crate) fn is_file_media(path: &String) -> bool {
    let p = Path::new(path);
    let ext = p.extension()
        .and_then(OsStr::to_str);
    let Some(ext) = ext else {
        debug!("No extension");
        return false;
    };
    // todo: not supported gif
    let file_extensions_with_efix = ["jpg", "jpeg", "heic", "png", "tiff", "tif", "webp"];
    file_extensions_with_efix.contains(&ext.to_string().to_ascii_lowercase().as_str())
}

/// `yyyy/mm/dd-hh-mm-ss[-i].ext`
/// OR `undated/checksum.ext`
pub(crate) fn get_desired_path(
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
                date_dir = format!("{}/{:0>2}", dt.year(), dt.month());
                name = format!(
                    "{:0>2}-{:0>2}{:0>2}{:0>2}",
                    dt.day(),
                    dt.hour(),
                    dt.minute(),
                    dt.second()
                );
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
async fn test_dt() {
    crate::test_util::setup_log().await;
    let dt = parse_exif_datetime(&Some("2017:08:19 10:21:59".to_string())).unwrap();
    assert_eq!(dt, "2017-08-19T10:21:59Z");
    let dt = parse_exif_datetime(&Some("2017:08:19".to_string())).unwrap();
    assert_eq!(dt, "2017-08-19T00:00:00Z");
}

#[tokio::test()]
async fn test_dt2() {
    crate::test_util::setup_log().await;
    let dt = parse_exif_datetime(&Some("2019:04:04 18:04:98".to_string())).unwrap();
    assert_eq!(dt, "2019-04-04T18:05:38Z");
}

#[tokio::test()]
async fn test_d1() {
    crate::test_util::setup_log().await;
    let d = parse_exif_date(&Some("2019:04:04".to_string())).unwrap();
    assert_eq!(d, "2019-04-04");
}

#[tokio::test()]
async fn test_parse_exif_created() {
    let m = MediaFromFileSystem::new("test/Canon_40D.jpg".to_string());
    let p = parse_exif(&m).unwrap();
    assert_eq!(
        p.datetime_original,
        Some("2008-05-30T15:56:01Z".to_string())
    );
}

#[tokio::test()]
async fn test_parse_exif_all_tags() {
    crate::test_util::setup_log().await;
    let p = Path::new("test/test1.jpg").to_path_buf();
    let t = all_tags(&p).unwrap();
    assert_eq!(t.get("GPS date"), Some(&"2017:08:18".to_string()));
}
