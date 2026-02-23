use crate::fs::FileSystem;
use serde::{Deserialize, Serialize};
use std::io::Read;
use tracing::{debug, warn};

pub(crate) fn detect_supplemental_info(
    path: &String,
    container: &dyn FileSystem,
) -> Option<String> {
    let google_supp_json_exts = vec![
        ".supplemental-metadata.json",
        ".supplemental-metad.json",
        ".suppl.json",
    ];
    for supp_json_ext in google_supp_json_exts {
        let supp_info_path = format!("{}{}", &path, supp_json_ext);
        if container.exists(&supp_info_path) {
            return Some(supp_info_path);
        }
    }
    None
}

pub(crate) fn load_supplemental_info(
    path: &String,
    container: &dyn FileSystem,
) -> Option<PsSupplementalInfo> {
    let reader_r = container.open(path);
    let Ok(reader) = reader_r else {
        warn!("Could not read supplemental json file: {path}");
        return None;
    };
    debug!("  Loaded: {path}");
    parse_supplemental_info(reader)
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfoGeoData {
    latitude: Option<f64>,
    longitude: Option<f64>,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfoPerson {
    pub(crate) name: Option<String>,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfoDateTime {
    timestamp: Option<String>, // actually a unix timestamp in seconds eg, 1716539968
    pub(crate) formatted: Option<String>,
}

impl SupplementalInfoDateTime {
    pub(crate) fn timestamp_s_as_iso_8601(&self) -> Option<String> {
        if let Some(ts) = &self.timestamp
            && let Ok(ts_i64) = ts.parse::<i64>()
        {
            if ts.len() == 10 {
                // seconds to milliseconds
                return crate::util::timestamp_to_rfc3339(ts_i64 * 1000);
            }
            return crate::util::timestamp_to_rfc3339(ts_i64);
        }
        None
    }
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct PsSupplementalInfo {
    pub(crate) geo_data: Option<SupplementalInfoGeoData>,
    pub(crate) geo_data_exif: Option<SupplementalInfoGeoData>,
    pub(crate) people: Vec<SupplementalInfoPerson>,
    pub(crate) photo_taken_time: Option<SupplementalInfoDateTime>,
    pub(crate) creation_time: Option<SupplementalInfoDateTime>,
}

fn parse_supplemental_info<R: Read>(json_reader: R) -> Option<PsSupplementalInfo> {
    let gs_r: Result<PsSupplementalInfo, _> = serde_json::from_reader(json_reader);
    if let Ok(gs) = gs_r {
        return Some(gs);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_parse_supp() -> anyhow::Result<()> {
        use anyhow::anyhow;
        crate::test_util::setup_log();
        use std::path::Path;
        let file = Path::new("test/test1.jpeg.supplemental-metadata.json");
        let json_reader = File::open(file)?;
        let r = parse_supplemental_info(json_reader).ok_or_else(|| anyhow!("Failed to parse supplemental info"))?;
        // long lat limited to 6 decimal places
        let latitude = r.geo_data.clone().ok_or_else(|| anyhow!("Missing geo_data"))?.latitude.ok_or_else(|| anyhow!("Missing latitude"))?;
        let longitude = r.geo_data.clone().ok_or_else(|| anyhow!("Missing geo_data"))?.longitude.ok_or_else(|| anyhow!("Missing longitude"))?;
        assert_eq!(format!("{latitude:.4}"), "-21.6303".to_string());
        assert_eq!(format!("{longitude:.4}"), "152.2605".to_string());
        let p = r.people.clone().first().ok_or_else(|| anyhow!("Missing person"))?.clone();
        assert_eq!(p.name.ok_or_else(|| anyhow!("Missing name"))?, "Tim Tam");
        let ct = r.creation_time.ok_or_else(|| anyhow!("Missing creation_time"))?;
        assert_eq!(ct.formatted.ok_or_else(|| anyhow!("Missing formatted date"))?, "24 May 2024, 08:39:28 UTC");
        assert_eq!(ct.timestamp.ok_or_else(|| anyhow!("Missing timestamp"))?, "1716539968");
        Ok(())
    }
}
