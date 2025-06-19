use std::fs;
use std::path::Path;
use anyhow::Context;
use serde_json::Value;
use tracing::{debug, info, warn};
use crate::upload::FsFile;

async fn detect_extra_info(ff: &FsFile) -> anyhow::Result<Option<String>> {
    let google_supp_json_exts = vec![
        ".supplemental-metadata.json",
        ".supplemental-metad.json",
        ".suppl.json",
    ];
    for supp_json_ext in google_supp_json_exts {
        let n = format!("{}{}", &ff.path, supp_json_ext);
        let supp_info_path = Path::new(&n);
        if supp_info_path.exists() {
            debug!("Found google supplemental metadata file: {:?}", supp_info_path);
            let s = fs::read_to_string(supp_info_path)
                .with_context(|| format!("Unable to read file: {:?}", supp_info_path))?;
            let j: Result<Value, _> = serde_json::from_str(&s);
            if let Ok(j) = j {
                let c = serde_json::to_string(&j);
                if let Ok(c) = c {
                    info!("Found supplemental metadata: {:?}", supp_info_path);
                    return Ok(Some(c));
                } else {
                    warn!("Unable to encode extra info JSON: {:?}", supp_info_path);
                    // continue and try others
                }
            } else {
                warn!("Unable to decode extra info JSON: {:?}", supp_info_path);
                // continue and try others
            }
        }
    }
    Ok(None)
}
