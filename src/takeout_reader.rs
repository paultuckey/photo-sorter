use crate::media_file::{PsFileFormat, guess_file_format, media_file_info_from_readable};
use crate::util::{MediaFileReadable, MediaFromFileSystem};
use anyhow::{Context, anyhow};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::debug;
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
    let mut count = 0;
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
    debug!("Counted {} files in zip {:?}", count, s);
    Ok(files)
}

pub(crate) fn analyze(file: &String, zip_file: &str, dry_run: &bool) -> anyhow::Result<PsFileInZip> {
    debug!("Analyzing {:?}", file);
    let media_from_zip = MediaFromZip::new(zip_file.to_string(), file.clone());
    let media_file_info_res = media_file_info_from_readable(&media_from_zip);
    let Ok(media_file) = media_file_info_res else {
        debug!("File unsupported: {:?}", file);
        return Err(anyhow!("Unsupported file format: {:?}", file));
    };
    debug!("file format detected {:?}", media_file.file_format);
    Ok(PsFileInZip {
        path: file.to_string().clone(),
        file_format: media_file.file_format.clone(),
    })
}
