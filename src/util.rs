use std::fs;
use anyhow::{Context, anyhow};
use std::fs::{File, ReadDir};
use std::io::Read;
use std::path::Path;
use tracing::debug;

pub(crate) fn checksum_file(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    checksum_bytes(&bytes)
}

pub(crate) fn checksum_from_read(media_file_reader: &dyn PsReadable) -> anyhow::Result<String> {
    let bytes = media_file_reader.to_bytes()?;
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

pub trait PsContainer {
    fn scan(&self) -> Vec<Box<dyn PsReadable>>;
    fn readable(&self, path: &String) -> Box<dyn PsReadable>;
}

pub struct PsDirectoryContainer {
    root: String,
}

impl PsDirectoryContainer {
    pub fn new(root: String) -> Self {
        PsDirectoryContainer { root }
    }
}

/// Recursively scans the directory and its subdirectories,
fn scan_dir_recursively(files: &mut Vec<Box<dyn PsReadable>>, dir_path: &Path) {
    if !dir_path.exists() || !dir_path.is_dir() {
        return;
    }
    let Ok(dir_reader) = fs::read_dir(dir_path) else {
        debug!("Unable to read directory: {:?}", dir_path);
        return;
    };
    for dir_entry in dir_reader {
        let Ok(dir_entry) = dir_entry else {
            debug!("Unable to read directory entry");
            continue;
        };
        let path = dir_entry.path();
        if !path.exists() {
            continue;
        }
        if path.is_file() {
            files.push(Box::new(PsDirectoryReadable::new(
                path.to_string_lossy().to_string(),
            )));
        } else if path.is_dir() {
            scan_dir_recursively(files, &path);
        }
    }
}

impl PsContainer for PsDirectoryContainer {
    fn scan(&self) -> Vec<Box<dyn PsReadable>> {
        let mut files = Vec::new();
        let root_path = Path::new(&self.root);
        if !root_path.exists() {
            debug!("Root path does not exist: {:?}", root_path);
            return files;
        }
        if !root_path.is_dir() {
            debug!("Root path is not a directory: {:?}", root_path);
            return files;
        }
        scan_dir_recursively(&mut files, &root_path);
        files
    }
    fn readable(&self, path: &String) -> Box<dyn PsReadable> {
        Box::new(PsDirectoryReadable::new(path.clone()))
    }
}

pub trait PsReadable {
    fn exists(&self) -> bool;
    fn to_bytes(&self) -> anyhow::Result<Vec<u8>>;
    /// grab the first `limit` bytes from the file, return empty vec if file is empty
    fn take(&self, limit: u64) -> anyhow::Result<Vec<u8>>;
    fn name(&self) -> String;
}

pub struct PsDirectoryReadable {
    file: String,
}

impl PsDirectoryReadable {
    pub fn new(file: String) -> Self {
        PsDirectoryReadable { file }
    }
}

impl PsReadable for PsDirectoryReadable {
    fn exists(&self) -> bool {
        let path = Path::new(&self.file);
        path.exists()
    }

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
