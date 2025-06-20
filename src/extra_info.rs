use serde_json::Value;
use tracing::{debug, info, warn};
use crate::util::{PsContainer, PsReadable};

pub(crate) fn detect_extra_info(path: &String, container: &Box<dyn PsContainer>) -> Option<String> {
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

fn read_extra_info(readable: &dyn PsReadable) -> Option<String> {
    let bytes = readable.to_bytes();
    let Ok(bytes) = bytes else {
        warn!("Unable to read bytes from {}", readable.name());
        return None;
    };
    let supp_info_path = readable.name();
    let j: Result<Value, _> = serde_json::from_slice(&bytes);
    if let Ok(j) = j {
        let c = serde_json::to_string(&j);
        if let Ok(c) = c {
            debug!("Found supplemental metadata: {:?}", supp_info_path);
            Some(c)
        } else {
            warn!("Unable to encode extra info JSON: {:?}", supp_info_path);
            None
        }
    } else {
        warn!("Unable to decode extra info JSON: {:?}", supp_info_path);
        None
    }
}
