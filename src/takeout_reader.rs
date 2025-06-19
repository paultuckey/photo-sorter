use crate::media_file::{MediaFileReadable, PsFileFormat, guess_file_format};
use anyhow::{Context, anyhow};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{debug};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub(crate) struct PsFileInZip {
    pub(crate) path: String,
    pub(crate) file_format: PsFileFormat,
}

pub struct MediaFromZip {
    zip_file: String,
    file: String,
}
impl MediaFromZip {
    pub fn new(zip_file: String, file: String) -> Self {
        MediaFromZip { zip_file, file }
    }
}
impl MediaFileReadable for MediaFromZip {
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
        let mut handle = file.take(limit);
        handle.read(&mut buffer)?;
        Ok(buffer)
    }

    fn name(&self) -> String {
        self.file.clone()
    }
}

pub(crate) fn scan(s: &str) -> anyhow::Result<Vec<PsFileInZip>> {
    let zip_path = Path::new(s);
    let mut files: Vec<String> = vec![];
    {
        let zip_file = File::open(zip_path) //
            .with_context(|| format!("Unable to open file {:?}", zip_path))?;
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
            files.push(file_name.clone().to_string());
        }
    }

    let mut ps_files: Vec<PsFileInZip> = vec![];
    for file_name  in files {
        let file_res = MediaFromZip::new(s.clone().to_string(), file_name.clone());
        let media_file = guess_file_format(&file_res);
        if media_file == PsFileFormat::Unsupported {
            debug!("File unsupported: {:?}", file_name);
            continue;
        }
        debug!("File {:?} {}", media_file, file_name);
        ps_files.push(PsFileInZip {
            path: file_name.to_string().clone(),
            file_format: media_file.clone(),
        });
    }
    Ok(ps_files)
}

pub(crate) fn count(s: &String) -> anyhow::Result<u32> {
    let zip_path = Path::new(s);
    let zip_file = File::open(zip_path) //
        .with_context(|| format!("Unable to open file {:?}", zip_path))?;

    let mut zip_archive =
        ZipArchive::new(zip_file) //
            .with_context(|| format!("Unable to open zip file {:?}", zip_path))?;
    let mut count = 0;
    for i in 0..zip_archive.len() {
        let file_res = zip_archive.by_index(i);
        let Some(file) = file_res.ok() else {
            continue;
        };
        if file.is_file() {
            count += 1;
        };
    }
    Ok(count)
}
