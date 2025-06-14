use std::path::Path;

pub(crate) fn checksum_file(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    checksum_bytes(&bytes)
}

pub(crate) fn checksum_string(s: &String) -> anyhow::Result<String> {
    let bytes = &s.as_bytes().to_vec();
    checksum_bytes(bytes)
}

pub(crate) fn checksum_bytes(bytes: &Vec<u8>) -> anyhow::Result<String> {
    let hash = sha256::digest(bytes);
    Ok(base64_url::encode(hash.as_bytes()))
}


