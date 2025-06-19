use std::fs::File;
use std::path::Path;
use anyhow::Context;
use crate::media_file::MediaFileReadable;

pub(crate) fn checksum_file(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    checksum_bytes(&bytes)
}

pub(crate) fn checksum_from_read(media_file_reader: &dyn MediaFileReadable) -> anyhow::Result<String> {
    let bytes= media_file_reader.to_bytes()?;
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

pub(crate) fn reader_from_path_string(input: &String) -> anyhow::Result<File> {
    let path = Path::new(input);
    let file = File::open(path) //
        .with_context(|| format!("Unable to open file {:?}", path))?;
    Ok(file)
}
