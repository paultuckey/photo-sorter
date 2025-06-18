use crate::media_file::guess_file_format;
use std::fs::File;
use std::path::Path;
use tracing::info;
use zip::ZipArchive;

pub(crate) fn scan(s: &str) -> anyhow::Result<()> {
    let zip_path = Path::new(s);
    let zip_file = File::open(zip_path)?;

    let mut zip_archive = ZipArchive::new(zip_file)?;

    for i in 0..zip_archive.len() {
        let file = zip_archive.by_index(i).unwrap();
        if !file.is_dir() {
            continue;
        }
        let Some(enclosed_name) = file.enclosed_name() else {
            continue;
        };
        let p = enclosed_name.as_path();
        let file_name = p.to_str().unwrap();
        let media_file_o = guess_file_format(file, file_name);
        println!("File {:?} {}", media_file_o, file_name);
    }
    Ok(())
}

pub(crate) fn count(s: &String) -> anyhow::Result<u32> {
    let zip_path = Path::new(s);
    let zip_file = File::open(zip_path)?;

    let mut archive = ZipArchive::new(zip_file)?;
    let mut count: u32 = 0;
    info!("Archive {} contains {} files", s, archive.len());
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.is_file() {
            count += 1;
        };
    }
    Ok(count)
}
