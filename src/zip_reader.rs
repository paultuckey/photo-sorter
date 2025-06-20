use crate::media::media_file_info_from_readable;
use crate::util::{PsContainer, PsReadable};
use anyhow::{Context, anyhow};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{debug, error, warn};
use zip::ZipArchive;

pub struct PsZipContainer {
    zip_file: String,
}

impl PsZipContainer {
    pub(crate) fn new(zip_file: String) -> Self {
        PsZipContainer { zip_file }
    }
}

impl PsContainer for PsZipContainer {
    fn scan(&self) -> Vec<Box<dyn PsReadable>> {
        let mut files = vec![];
        let zip_path = Path::new(&self.zip_file);
        let Ok(zip_file) = File::open(zip_path) else {
            error!("Unable to open file {:?}", zip_path);
            return files;
        };
        let zip_archive = ZipArchive::new(zip_file);
        let Ok(mut zip_archive) = zip_archive else {
            error!("Unable to open zip file {:?}", zip_path);
            return files;
        };
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
            let file_name = p.to_str().unwrap();
            files.push(Box::new(PsZipReadable::new(
                self.zip_file.clone(),
                file_name.to_string(),
            )));
        }
        debug!("Counted {} files in zip {:?}", files.len(), self.zip_file);
        files
    }

    fn readable(&self, path: &String) -> Box<dyn PsReadable> {
        Box::new(PsZipReadable::new(self.zip_file.clone(), path.clone()))
    }
}

pub struct PsZipReadable {
    zip_file: String,
    file: String,
}

impl PsZipReadable {
    pub fn new(zip_file: String, file: String) -> Self {
        PsZipReadable { zip_file, file }
    }
}
impl PsReadable for PsZipReadable {
    fn exists(&self) -> bool {
        let zip_path = Path::new(&self.zip_file);
        let Ok(zip_file) = File::open(zip_path) else {
            warn!("Unable to open zip file {:?}", zip_path);
            return false;
        };
        let Ok(mut zip_archive) = ZipArchive::new(zip_file) else {
            warn!("Unable to open zip file {:?}", zip_path);
            return false;
        };
        let file_res = zip_archive.by_name(&self.file.clone());
        let Some(file) = file_res.ok() else {
            return false;
        };
        file.is_file()
    }

    fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let zip_path = Path::new(&self.zip_file);
        let zip_file = File::open(zip_path) //
            .with_context(|| format!("Unable to open file {:?}", zip_path))?;
        let mut zip_archive =
            ZipArchive::new(zip_file) //
                .with_context(|| format!("Unable to open zip file {:?}", zip_path))?;
        let file_in_zip_name = self.file.clone();
        let file_res = zip_archive.by_name(&file_in_zip_name);
        let Some(mut file) = file_res.ok() else {
            return Err(anyhow!("Unable to find file {:?}", file_in_zip_name));
        };
        if file.is_dir() {
            return Err(anyhow!("File is a dir {:?}", file_in_zip_name));
        }
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap_or(0);
        Ok(buffer)
    }

    fn take(&self, limit: u64) -> anyhow::Result<Vec<u8>> {
        let zip_path = Path::new(&self.zip_file);
        let zip_file = File::open(zip_path) //
            .with_context(|| format!("Unable to open file {:?}", zip_path))?;
        let mut zip_archive =
            ZipArchive::new(zip_file) //
                .with_context(|| format!("Unable to open zip file {:?}", zip_path))?;
        let file_in_zip_name = self.file.clone();
        let file_res = zip_archive.by_name(&file_in_zip_name);
        let Some(file) = file_res.ok() else {
            return Err(anyhow!("Unable to find file {:?}", file_in_zip_name));
        };
        if file.is_dir() {
            return Err(anyhow!("File is a dir {:?}", file_in_zip_name));
        }
        let mut buffer = Vec::new();
        let mut limited_reader = file.take(limit);
        limited_reader.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    fn name(&self) -> String {
        self.file.clone()
    }
}
