use std::collections::HashMap;
use log::{debug, info, warn};
use serde::Deserialize;
use crate::file_type::QuickScannedFile;
use crate::util::{PsContainer};

pub(crate) fn detect_supplemental_info(path: &String, container: &Box<dyn PsContainer>) -> Option<String> {
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

pub(crate) fn load_supplemental_info(qsf: &QuickScannedFile, container: &mut Box<dyn PsContainer>, json_hashmap: &mut HashMap<String, SupplementalInfo>) {
    let Some(path) = qsf.supplemental_json_file.clone() else {
        return;
    };
    let bytes = container.file_bytes(&path);
    let Ok(bytes) = bytes else {
        warn!("Could not read supplemental json file: {path}");
        return;
    };
    debug!("  Loaded: {path}");
    let s = String::from_utf8_lossy(&bytes).to_string();
    let si_o = parse_supplemental_info(s);
    if let Some(si) = si_o {
        json_hashmap.insert(path, si);
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfoGeoData {
    latitude: Option<f64>,
    longitude: Option<f64>,
}
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfoPerson {
    name: Option<String>,
}
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfoDateTime {
    timestamp: Option<String>, // actually a unix timestamp in seconds eg, 1716539968
    formatted: Option<String>,
}
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct SupplementalInfo {
    geo_data: Option<SupplementalInfoGeoData>,
    geo_data_exif: Option<SupplementalInfoGeoData>,
    people: Vec<SupplementalInfoPerson>,
    photo_taken_time: Option<SupplementalInfoDateTime>,
    creation_time: Option<SupplementalInfoDateTime>,
}

fn parse_supplemental_info(json: String) -> Option<SupplementalInfo> {
    let gs_r: Result<SupplementalInfo, _> = serde_json::from_str(&json);
    if let Ok(gs) = gs_r {
        return Some(gs);
    }
    None
}

fn lat_long_from_geo_data(geo_data: SupplementalInfoGeoData) -> Option<String> {
    if let Some(lat) = geo_data.latitude {
        if let Some(long) = geo_data.longitude {
            // only need 5 decimal places to get 50m acuracy
            return Some(format!("{:.6},{:.6}", lat, long));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test()]
    async fn test_parse_supp() -> anyhow::Result<()> {
        crate::test_util::setup_log().await;
        use std::path::Path;
        let json = Path::new("test/test1.jpeg.supplemental-metadata.json");
        let json_s = std::fs::read_to_string(json)?;
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