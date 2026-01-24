use chrono::DateTime;
use nom_exif::{MediaParser, MediaSource, TrackInfo, TrackInfoTag};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek};
use tracing::{info, warn};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct PsMp4Info {
    pub width: Option<u64>,
    pub height: Option<u64>,
    // rfc3339
    pub creation_time: Option<String>,
    pub duration_ms: Option<u64>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub software: Option<String>,
    pub author: Option<String>,
    pub gps_iso_6709: Option<String>,
}

pub fn extract_mp4_metadata<R: Read + Seek>(reader: R) -> Option<PsMp4Info> {
    let ms_r = MediaSource::seekable(reader);
    let Ok(ms) = ms_r else {
        warn!("Failed to read MP4 media source");
        return None;
    };
    if !ms.has_track() {
        return None;
    }
    let mut parser = MediaParser::new();
    let info: nom_exif::Result<TrackInfo> = parser.parse(ms);

    match info {
        Err(e) => {
            warn!("Failed to parse MP4 metadata: {:?}", e);
            None
        }
        Ok(info) => {
            let pm = PsMp4Info {
                width: parse_to_o_u64(&info.get(TrackInfoTag::ImageWidth)),
                height: parse_to_o_u64(&info.get(TrackInfoTag::ImageHeight)),
                creation_time: parse_to_o_s(&info.get(TrackInfoTag::CreateDate)),
                duration_ms: parse_to_o_u64(&info.get(TrackInfoTag::DurationMs)),
                make: parse_to_o_s(&info.get(TrackInfoTag::Make)),
                model: parse_to_o_s(&info.get(TrackInfoTag::Model)),
                software: parse_to_o_s(&info.get(TrackInfoTag::Software)),
                author: parse_to_o_s(&info.get(TrackInfoTag::Author)),
                gps_iso_6709: parse_to_o_s(&info.get(TrackInfoTag::GpsIso6709)),
            };
            info.iter()
                // filter out known tags from above
                .filter(|(tag, _)| {
                    !matches!(
                        tag,
                        TrackInfoTag::ImageWidth
                            | TrackInfoTag::ImageHeight
                            | TrackInfoTag::CreateDate
                            | TrackInfoTag::DurationMs
                            | TrackInfoTag::Make
                            | TrackInfoTag::Model
                            | TrackInfoTag::Software
                            | TrackInfoTag::Author
                            | TrackInfoTag::GpsIso6709
                    )
                })
                .for_each(|info| {
                    info!("MP4 Additional Metadata: {} = {}", info.0, info.1);
                });
            Some(pm)
        }
    }
}

fn parse_to_o_u64(opt: &Option<&nom_exif::EntryValue>) -> Option<u64> {
    if let Some(v) = opt
        && let Ok(s) = v.to_string().parse::<u64>()
    {
        return Some(s);
    }
    None
}

fn parse_to_o_s(opt: &Option<&nom_exif::EntryValue>) -> Option<String> {
    let Some(v) = opt else {
        return None;
    };
    Some(v.to_string())
}

fn parse_date_to_o_ms(opt: &Option<&nom_exif::EntryValue>) -> Option<i64> {
    let Some(v) = opt else {
        return None;
    };
    let Ok(dt) = DateTime::parse_from_rfc3339(&v.to_string()) else {
        return None;
    };
    Some(dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{PsContainer, PsDirectoryContainer};
    use std::path::Path;

    #[test]
    fn test_parse_mp4() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let reader = c.file_reader(&"Hello.mp4".to_string())?;
        let meta = extract_mp4_metadata(reader).unwrap();
        assert_eq!(meta.width, Some(854));
        assert_eq!(meta.height, Some(480));
        assert_eq!(meta.duration_ms, Some(5000));
        assert_eq!(
            meta.creation_time,
            Some("2024-04-18T11:24:26+00:00".to_string())
        );
        Ok(())
    }

    /// For research scal all MP4 files in input/ directory and look for unknown tags
    #[test]
    #[ignore]
    fn test_all_mp4s() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new(&"input".to_string());
        for si in c.scan() {
            let path = Path::new(&si.file_path);
            if path
                .extension()
                .map_or(false, |ext| ext.eq_ignore_ascii_case("mp4"))
            {
                let reader = c.file_reader(&path.to_string_lossy().to_string())?;
                let _ = extract_mp4_metadata(reader);
            }
        }
        Ok(())
    }
}
