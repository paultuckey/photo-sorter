use re_mp4::{Mp4, Track, TrackId, TrackKind};
use std::collections::BTreeMap;
use std::io::{BufReader, Cursor};

#[derive(Debug, Clone)]
pub(crate) struct ParsedMp4 {
    pub width: u64,
    pub height: u64,
    pub creation_date: Option<i64>,
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
    let creation_date = mp4_time_to_epoch_ms(mp4.moov.mvhd.creation_time);
    Ok(ParsedMp4 {
        width,
        height,
        creation_date,
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
