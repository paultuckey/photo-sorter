use crate::media::media_file_info_from_readable;
use crate::util::PsReadableFile;
use anyhow::{Context, anyhow};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::debug;
use zip::ZipArchive;

pub struct PsReadableFromZip {
    zip_file: String,
    file: String,
}
impl PsReadableFromZip {
    pub fn new(zip_file: String, file: String) -> Self {
        PsReadableFromZip { zip_file, file }
    }
}
impl PsReadableFile for PsReadableFromZip {
    fn another(&self, path: &String) -> Box<dyn PsReadableFile> {
        Box::new(PsReadableFromZip::new(self.zip_file.clone(), path.clone()))
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

pub(crate) fn scan(s: &String) -> anyhow::Result<Vec<String>> {
    let zip_path = Path::new(s);
    let zip_file = File::open(zip_path) //
        .with_context(|| format!("Unable to open file {:?}", zip_path))?;
    let mut files = vec![];
    let mut zip_archive =
        ZipArchive::new(zip_file) //
            .with_context(|| format!("Unable to open zip file {:?}", zip_path))?;
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
        files.push(file_name.to_string());
    }
    debug!("Counted {} files in zip {:?}", files.len(), s);
    Ok(files)
}
