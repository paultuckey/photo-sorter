use crate::util::PsContainer;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

pub(crate) fn detect_supplemental_info(
    path: &String,
    container: &dyn PsContainer,
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
    container: &mut Box<dyn PsContainer>,
) -> Option<SupplementalInfo> {
    let bytes = container.file_bytes(path);
    let Ok(bytes) = bytes else {
        warn!("Could not read supplemental json file: {path}");
        return None;
    };
    debug!("  Loaded: {path}");
    let s = String::from_utf8_lossy(&bytes).to_string();
    parse_supplemental_info(s)
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
    pub(crate) fn timestamp_as_epoch_ms(&self) -> Option<i64> {
        if let Some(ts) = &self.timestamp
            && let Ok(ts_i64) = ts.parse::<i64>()
        {
            if ts.len() == 10 {
                // seconds to milliseconds
                return Some(ts_i64 * 1000);
            }
            return Some(ts_i64);
        }
        None
    }
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfo {
    pub(crate) geo_data: Option<SupplementalInfoGeoData>,
    pub(crate) geo_data_exif: Option<SupplementalInfoGeoData>,
    pub(crate) people: Vec<SupplementalInfoPerson>,
    pub(crate) photo_taken_time: Option<SupplementalInfoDateTime>,
    pub(crate) creation_time: Option<SupplementalInfoDateTime>,
}

fn parse_supplemental_info(json: String) -> Option<SupplementalInfo> {
    let gs_r: Result<SupplementalInfo, _> = serde_json::from_str(&json);
    if let Ok(gs) = gs_r {
        return Some(gs);
    }
    None
}

fn lat_long_from_geo_data(geo_data: SupplementalInfoGeoData) -> Option<String> {
    if let Some(lat) = geo_data.latitude
        && let Some(long) = geo_data.longitude
    {
        // only need 5 decimal places to get 50m acuracy
        return Some(format!("{lat:.6},{long:.6}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_supp() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        use std::path::Path;
        let file = Path::new("test/test1.jpeg.supplemental-metadata.json");
        let json_s = std::fs::read_to_string(file)?;
        let r = parse_supplemental_info(json_s).unwrap();
        // long lat limited to 6 decimal places
        let latitude = r.geo_data.clone().unwrap().latitude.unwrap();
        let longitude = r.geo_data.clone().unwrap().longitude.unwrap();
        assert_eq!(format!("{latitude:.4}"), "-21.6303".to_string());
        assert_eq!(format!("{longitude:.4}"), "152.2605".to_string());
        let p = r.people.clone().first().unwrap().clone();
        assert_eq!(p.name.unwrap(), "Tim Tam");
        let ct = r.creation_time.unwrap();
        assert_eq!(ct.formatted.unwrap(), "24 May 2024, 08:39:28 UTC");
        assert_eq!(ct.timestamp.unwrap(), "1716539968");
        Ok(())
    }
}
