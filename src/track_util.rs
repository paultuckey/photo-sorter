use nom_exif::{MediaParser, MediaSource, TrackInfo, TrackInfoTag};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek};
use tracing::{info, warn};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct PsTrackInfo {
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

pub fn parse_track_info<R: Read + Seek>(reader: R) -> Option<PsTrackInfo> {
    let ms_r = MediaSource::seekable(reader);
    let Ok(ms) = ms_r else {
        warn!("Failed to read track media source");
        return None;
    };
    if !ms.has_track() {
        return None;
    }
    let mut parser = MediaParser::new();
    let info: nom_exif::Result<TrackInfo> = parser.parse(ms);

    match info {
        Err(e) => {
            warn!("Failed to parse track metadata: {:?}", e);
            None
        }
        Ok(info) => {
            let ti = PsTrackInfo {
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
                    info!("Track Additional Metadata: {} = {}", info.0, info.1);
                });
            Some(ti)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{PsContainer, PsDirectoryContainer};
    use std::path::Path;

    #[test]
    fn test_parse_track() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let reader = c.file_reader(&"Hello.mp4".to_string())?;
        let meta = parse_track_info(reader).unwrap();
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
                let _ = parse_track_info(reader);
            }
        }
        Ok(())
    }
}
