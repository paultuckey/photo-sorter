use anyhow::Context;
use anyhow::anyhow;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{debug, error};
use zip::ZipArchive;

pub(crate) fn checksum_file(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path)?;
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

pub trait PsContainer {
    fn scan(&self) -> Vec<String>;
    fn file_bytes(&mut self, path: &String) -> anyhow::Result<Vec<u8>>;
    fn exists(&self, path: &String) -> bool;
}

pub struct PsDirectoryContainer {
    root: String,
}

impl PsDirectoryContainer {
    pub fn new(root: String) -> Self {
        PsDirectoryContainer { root }
    }
    pub(crate) fn write(&self, dry_run: bool, path: &String, bytes: &Vec<u8>) {
        let p = Path::new(&self.root).join(path);
        if dry_run {
            debug!("Dry run: would write file {:?} with {} bytes", p, bytes.len());
            return;
        }
        if let Err(e) = fs::create_dir_all(p.parent().unwrap()) {
            error!("Unable to create directory {:?}: {}", p.parent(), e);
            return;
        }
        if let Err(e) = fs::write(&p, bytes) {
            error!("Unable to write file {:?}: {}", p, e);
            return;
        }
        debug!("Wrote file {:?}", p);
    }
}

/// Recursively scans the directory and its subdirectories,
fn scan_dir_recursively(files: &mut Vec<String>, dir_path: &Path, root_path: &Path) {
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
            // trim root path from the file path
            let relative_path = path.strip_prefix(root_path).unwrap_or(&path);
            files.push(relative_path.to_string_lossy().to_string());
        } else if path.is_dir() {
            scan_dir_recursively(files, &path, root_path);
        }
    }
}

impl PsContainer for PsDirectoryContainer {
    fn scan(&self) -> Vec<String> {
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
        scan_dir_recursively(&mut files, root_path, root_path);
        files
    }
    fn file_bytes(&mut self, path: &String) -> anyhow::Result<Vec<u8>> {
        let file_path = Path::new(&self.root).join(path);
        let mut file =
            File::open(&file_path) //
                .with_context(|| format!("Unable to open file {:?}", file_path))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap_or(0);
        Ok(buffer)
    }
    fn exists(&self, file: &String) -> bool {
        Path::new(&self.root).join(file).exists()
    }
}

pub struct PsZipContainer {
    zip_file: String,
    index: Vec<String>,
    zip: ZipArchive<File>,
}

impl PsZipContainer {
    pub(crate) fn new(zip_file: String) -> Self {
        let z = ZipArchive::new(File::open(zip_file.clone()).unwrap());
        let mut c = PsZipContainer {
            zip_file,
            index: vec![],
            zip: z.unwrap(),
        };
        c.index();
        c
    }
    fn index(&mut self) {
        // let zip_path = Path::new(&self.zip_file);
        // let Ok(zip_file) = File::open(zip_path) else {
        //     error!("Unable to open file {:?}", zip_path);
        //     return;
        // };
        let zip_archive = &mut self.zip; //::new(zip_file);
        // let Ok(mut zip_archive) = zip_archive else {
        //     error!("Unable to open zip file {:?}", zip_path);
        //     return;
        // };
        for i in 0..zip_archive.len() {
            let file_res = zip_archive.by_index(i);
            let Some(file) = file_res.ok() else {
                continue;
            };
            if file.is_dir() {
                continue;
            }
            let Some(enclosed_name) = file.enclosed_name() else {
                continue;
            };
            let p = enclosed_name.as_path();
            let file_name_o = p.to_str();
            let Some(file_name) = file_name_o else {
                continue;
            };
            self.index.push(file_name.to_string());
        }
        debug!(
            "Counted {} files in zip {:?}",
            self.index.len(),
            self.zip_file
        );
    }
}

impl PsContainer for PsZipContainer {
    fn scan(&self) -> Vec<String> {
        self.index.clone()
    }
    fn file_bytes(&mut self, path: &String) -> anyhow::Result<Vec<u8>> {
        let file_res = self.zip.by_name(path);
        let Some(mut file) = file_res.ok() else {
            return Err(anyhow!("Unable to find file {:?}", path));
        };
        if file.is_dir() {
            return Err(anyhow!("File is a dir {:?}", path));
        }
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap_or(0);
        Ok(buffer)
    }
    fn exists(&self, path: &String) -> bool {
        self.index.contains(path)
    }
}
