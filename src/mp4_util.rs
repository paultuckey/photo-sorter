use re_mp4::{Mp4, Track, TrackId, TrackKind};
use std::collections::BTreeMap;
use std::io::{BufReader, Cursor};

#[derive(Debug, Clone)]
pub(crate) struct ParsedMp4 {
    pub width: u64,
    pub height: u64,
    pub creation_time: Option<i64>,
    pub modified_time: Option<i64>,
    pub duration: u64,
    pub timescale: u32,
}

pub fn extract_mp4_metadata(bytes: &Vec<u8>) -> anyhow::Result<ParsedMp4> {
    let mut cursor = Cursor::new(bytes);
    let bufread_seek = BufReader::new(&mut cursor);
    let mp4 = Mp4::read(bufread_seek, bytes.len() as u64)?;
    let video_track = video_track(mp4.tracks());
    let (width, height) = if let Some(track) = video_track {
        (track.width as u64, track.height as u64)
    } else {
        (0, 0)
    };
    let creation_time = mp4_time_to_epoch_ms(mp4.moov.mvhd.creation_time);
    let modified_time = mp4_time_to_epoch_ms(mp4.moov.mvhd.modification_time);
    let duration = mp4.moov.mvhd.duration;
    let timescale = mp4.moov.mvhd.timescale;
    Ok(ParsedMp4 {
        width,
        height,
        creation_time,
        modified_time,
        duration,
        timescale,
    })
}

fn mp4_time_to_epoch_ms(mp4_time: u64) -> Option<i64> {
    use chrono::{Duration, TimeZone, Utc};
    if mp4_time == 0 {
        return None;
    }
    // MP4 creation time is based on seconds since 1904-01-01
    let utc =
        Utc.with_ymd_and_hms(1904, 1, 1, 0, 0, 0).unwrap() + Duration::seconds(mp4_time as i64);
    Some(utc.timestamp_millis())
}

fn video_track(tracks: &BTreeMap<TrackId, Track>) -> Option<&Track> {
    const NOT_VIDEO: TrackKind = TrackKind::Subtitle;
    tracks
        .iter()
        .find(|(_, t)| TrackKind::Video == t.kind.unwrap_or(NOT_VIDEO))
        .map(|(_, t)| t)
}

#[cfg(test)]
mod tests {
    use crate::util::{PsContainer, PsDirectoryContainer};
    use super::*;

    #[test]
    fn test_parse_mp4() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let bytes = c.file_bytes(&"Hello.mp4".to_string())?;
        let meta = extract_mp4_metadata(&bytes)?;
        assert_eq!(meta.width, 854);
        assert_eq!(meta.height, 480);
        assert_eq!(meta.duration, 5000);
        assert_eq!(meta.timescale, 1000);
        assert_eq!(meta.modified_time, Some(1713439466000));
        assert_eq!(meta.creation_time, Some(1713439466000));
        Ok(())
    }
}