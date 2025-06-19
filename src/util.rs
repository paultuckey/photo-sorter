use std::fs::File;
use std::io::Read;
use std::path::Path;
use anyhow::Context;
use tracing::debug;

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

pub trait MediaFileReadable {
    fn to_bytes(&self) -> anyhow::Result<Vec<u8>>;
    /// grab the first `limit` bytes from the file, return empty vec if file is empty
    fn take(&self, limit: u64) -> anyhow::Result<Vec<u8>>;
    fn name(&self) -> String;
}

pub struct MediaFromFileSystem {
    file: String,
}
impl MediaFromFileSystem {
    pub fn new(file: String) -> Self {
        MediaFromFileSystem { file }
    }
}
impl MediaFileReadable for MediaFromFileSystem {
    fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let path = Path::new(&self.file);
        let mut file = File::open(path) //
            .with_context(|| format!("Unable to open file {:?}", path))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap_or(0);
        Ok(buffer)
    }

    fn take(&self, limit: u64) -> anyhow::Result<Vec<u8>> {
        let path = Path::new(&self.file);
        let file = File::open(path) //
            .with_context(|| format!("Unable to open file {:?}", path))?;
        let mut buffer = Vec::new();
        let mut limited_reader = file.take(limit); // Limit to 1000 bytes
        limited_reader.read_to_end(&mut buffer)?;
        debug!("Read {} bytes from file {:?}", buffer.len(), self.file);
        Ok(buffer)
    }

    fn name(&self) -> String {
        self.file.clone()
    }
}
