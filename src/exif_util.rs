use chrono::DateTime;
use nom_exif::{ExifIter, ExifTag, MediaParser, MediaSource, ParsedExifEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek};
use tracing::{debug, warn};

/*

Util file to help with exif parsing.

it's not the responsibility of this module to decide if exif data is valid or not, just to
parse it best as possible.

store in db as json

 */

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct PsExifInfo {
    // dates as ISO 8601
    pub(crate) tags: HashMap<String, String>,
    // as iso6709
    pub(crate) gps: Option<String>,
}

pub(crate) fn parse_exif_info<R: Read + Seek>(reader: R) -> Option<PsExifInfo> {
    let ms = MediaSource::seekable(reader);
    let Ok(ms) = ms else {
        debug!("Could not create MediaSource");
        return None;
    };
    if !ms.has_exif() {
        debug!("File does not mave exif metadata");
        return None;
    }
    let mut m = HashMap::new();
    let mut parser = MediaParser::new();
    let exif_iter_r: nom_exif::Result<ExifIter> = parser.parse(ms);
    let mut ps_gps_info = None;
    match exif_iter_r {
        Ok(exif_iter) => {
            for entry in exif_iter.clone() {
                let Some(tag_enum) = entry.tag() else {
                    continue; // skip unrecognised tags
                };
                let tag_name = tag_enum.to_string();
                let s_o = field_to_opt_string(&entry);
                let Some(s) = s_o else {
                    continue; // only support tags with value
                };
                if s.len() > 1024 {
                    continue; // skip large values
                }
                m.insert(tag_name, s);
            }
            if let Some(gps_info) = exif_iter
                .parse_gps_info()
                .ok()
                .flatten()
                .map(|g| g.format_iso6709())
            {
                ps_gps_info = Some(gps_info);
            }
        }
        Err(e) => {
            warn!("Could not read EXIF data: {e}");
        }
    }
    Some(PsExifInfo {
        tags: m,
        gps: ps_gps_info,
    })
}

fn field_to_opt_string(field: &ParsedExifEntry) -> Option<String> {
    if let Ok(value) = field.clone().take_result() {
        match value {
            nom_exif::EntryValue::Undefined(_) => {
                // skip undefined values
                return None;
            }
            _ => {
                // dates are returned as a ISO 8601 string with no timezone
                return Some(value.to_string());
            }
        }
    }
    None
}

fn field_value(exif: &PsExifInfo, code: ExifTag) -> Option<String> {
    exif.tags.get(&code.to_string()).cloned()
}

pub(crate) fn best_guess_taken_exif(exif: &Option<PsExifInfo>) -> Option<String> {
    match exif {
        Some(exif) => {
            if let Some(dt) = field_value(exif, ExifTag::DateTimeOriginal) {
                return Some(dt);
            }
            if let Some(dt) = field_value(exif, ExifTag::ModifyDate) {
                return Some(dt);
            }
            if let Some(dt) = field_value(exif, ExifTag::GPSDateStamp) {
                return Some(dt);
            }
            None
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::PsContainer;
    use crate::util::PsDirectoryContainer;

    #[test]
    fn test_parse_exif_mp4() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new(&"test".to_string());
        let reader = c.file_reader(&"Hello.mp4".to_string())?;
        let t = parse_exif_info(reader);
        assert_eq!(t.is_none(), true);
        Ok(())
    }

    #[test]
    fn test_parse_exif_all_tags() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let mut c = PsDirectoryContainer::new("test");
        let reader = c.file_reader("Canon_40D.jpg")?;
        let t = parse_exif_info(reader).unwrap().tags;
        assert_eq!(t.len(), 44);
        let mut tag_names: Vec<String> = t
            .iter()
            .map(|(t, _)| t.to_string())
            .collect();
        assert_eq!(
            tag_names.sort(),
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
            .sort()
        );

        let make_tag_value = t.get(&ExifTag::Make.to_string()).unwrap();
        assert_eq!(make_tag_value, &"Canon".to_string());

        // SubSecTimeOriginal
        let sub_sec_time_original = t.get(&ExifTag::SubSecTimeOriginal.to_string()).unwrap();
        assert_eq!(sub_sec_time_original.clone(), "00".to_string());
        Ok(())
    }
}
