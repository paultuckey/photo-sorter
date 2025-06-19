use crate::exif_util::{ParsedExif, best_guess_taken_dt, get_desired_path, parse_exif};
use crate::util::{checksum_file, checksum_from_read, reader_from_path_string};
use std::fs::File;
use std::io::{ErrorKind, Read, Seek};
use std::path::Path;
use anyhow::{anyhow, Context};
use tracing::debug;
use tracing::log::warn;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PsFileFormat {
    Jpg,
    Png,
    Heic,
    Gif,
    Mp4,
    Json,
    Csv,
    Unsupported,
}

#[derive(Debug, Clone)]
pub(crate) struct MediaFileInfo {
    pub(crate) original_path: String,
    pub(crate) file_format: PsFileFormat,
    pub(crate) parsed_exif: Option<ParsedExif>,
    pub(crate) checksum: Option<String>,
    pub(crate) desired_path: Option<String>,
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
        let mut handle = file.take(limit);
        handle.read(&mut buffer)?;
        Ok(buffer)
    }

    fn name(&self) -> String {
        self.file.clone()
    }
}

pub(crate) fn media_file_info_from_readable(reader: &dyn MediaFileReadable) -> anyhow::Result<MediaFileInfo> {
    let input = reader.name();
    let exif_o = parse_exif(reader);
    let checksum_o = checksum_from_read(reader).ok();
    let guessed_ff = guess_file_format(reader);

    if guessed_ff == PsFileFormat::Unsupported {
        debug!("File {:?} is not a valid media file", input);
        return Err(anyhow!("File is not a valid media file"));
    }
    let ext = file_ext_from_file_format(&guessed_ff);

    let guessed_datetime = best_guess_taken_dt(&exif_o);
    let mut desired_path_o = None;
    match checksum_o.clone() {
        Some(checksum) => {
            desired_path_o = Some(get_desired_path(
                &checksum.clone(),
                &guessed_datetime,
                &ext,
                0,
            ));
        }
        None => {
            // could not calculate checksum, not a valid file
        }
    }
    let media_file_info = MediaFileInfo {
        original_path: input.clone(),
        file_format: guessed_ff.clone(),
        parsed_exif: exif_o.clone(),
        checksum: checksum_o.clone(),
        desired_path: desired_path_o.clone(),
    };
    Ok(media_file_info)
}

pub(crate) fn file_ext_from_file_format(ff: &PsFileFormat) -> String {
    match ff {
        PsFileFormat::Jpg => "jpg".to_string(),
        PsFileFormat::Gif => "gif".to_string(),
        PsFileFormat::Png => "png".to_string(),
        PsFileFormat::Heic => "heic".to_string(),
        PsFileFormat::Mp4 => "mp4".to_string(),
        PsFileFormat::Unsupported => "bin".to_string(),
        PsFileFormat::Json => "json".to_string(),
        PsFileFormat::Csv => "csv".to_string(),
    }
}

pub(crate) fn file_format_from_content_type(ct: &str) -> PsFileFormat {
    match ct {
        "image/jpeg" => PsFileFormat::Jpg,
        "image/gif" => PsFileFormat::Gif,
        "image/png" => PsFileFormat::Png,
        "image/heic" => PsFileFormat::Heic,
        "video/mp4" => PsFileFormat::Mp4,
        "application/octet-stream" => PsFileFormat::Unsupported,
        "application/json" => PsFileFormat::Unsupported,
        "text/csv" => PsFileFormat::Csv,
        _ => PsFileFormat::Unsupported,
    }
}

pub(crate) fn guess_file_format(media_file_readable: &dyn MediaFileReadable) -> PsFileFormat {
    // Limit buffer size same as that inside `file_format` crate
    let buffer_res = media_file_readable.take(36_870); 
    let Ok(buffer) = buffer_res else {
        warn!("File {:?} is empty, cannot guess mime type", media_file_readable.name());
        return PsFileFormat::Unsupported;
    };
    if media_file_readable.name().to_lowercase().ends_with(".json") {
        return PsFileFormat::Json;
    }
    let fmt = file_format::FileFormat::from_bytes(buffer);
    let mt = fmt.media_type();
    if mt == "application/octet-stream" {
        warn!("Could not guess mime type for {:?}", media_file_readable.name());
    }
    file_format_from_content_type(&mt)
}
