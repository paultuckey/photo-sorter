use std::fs::File;
use crate::exif_util::{best_guess_taken_dt, get_desired_path, parse_exif, ParsedExif};
use std::io::{ErrorKind, Read, Seek};
use std::path::Path;
use tracing::debug;
use tracing::log::warn;
use crate::util::checksum_file;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PsFileFormat {
    Jpg,
    Gif,
    Png,
    Heic,
    Mp4,
    Bin,
    Json,
    Csv,
}

#[derive(Debug, Clone)]
pub(crate) struct MediaFileInfo {
    pub(crate) original_path: String,
    pub(crate) file_format: PsFileFormat,
    pub(crate) parsed_exif: Option<ParsedExif>,
    pub(crate) checksum: Option<String>,
    pub(crate) desired_path: Option<String>,
}

pub(crate) fn media_file_info_from_path(input: &String) -> Option<MediaFileInfo> {
    let p = Path::new(input);
    let f = File::open(p).unwrap(); // todo
    let guessed_ff = guess_file_format(f, input.as_str());
    if guessed_ff == PsFileFormat::Bin {
        debug!("File {:?} is not a valid media file", p);
        return None;
    }
    let exif_o = parse_exif(p);
    let ext = file_ext_from_file_format(&guessed_ff);
    let guessed_datetime = best_guess_taken_dt(&exif_o);
    let checksum_o = checksum_file(&p).ok();
    let mut desired_path_o = None;
    match checksum_o.clone() {
        Some(checksum) => {
            desired_path_o = Some(get_desired_path(&checksum.clone(), &guessed_datetime, &ext, 0));
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
    Some(media_file_info)
}

pub(crate) fn file_ext_from_file_format(ff: &PsFileFormat) -> String {
    match ff {
        PsFileFormat::Jpg => "jpg".to_string(),
        PsFileFormat::Gif => "gif".to_string(),
        PsFileFormat::Png => "png".to_string(),
        PsFileFormat::Heic => "heic".to_string(),
        PsFileFormat::Mp4 => "mp4".to_string(),
        PsFileFormat::Bin => "bin".to_string(),
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
        "application/octet-stream" => PsFileFormat::Bin,
        "application/json" => PsFileFormat::Bin,
        "text/csv" => PsFileFormat::Csv,
        _ => PsFileFormat::Bin,
    }
}

pub(crate) fn guess_file_format<R: Read>(file: R, file_name: &str) -> PsFileFormat {
    let mut limited_reader = file.take(36_870); // Limit to 1000 bytes
    let mut buffer = Vec::new();
    

    // Read all bytes (up to the 1000 limit) into the buffer
    let num_bytes_read = limited_reader.read_to_end(&mut buffer);
    match num_bytes_read {
        Ok(n) => {
            if n == 0 {
                warn!("File {:?} is empty, cannot guess mime type", file_name);
                return PsFileFormat::Bin;
            }
            if file_name.to_lowercase().ends_with(".json") {
                return PsFileFormat::Json;
            }
            let fmt = file_format::FileFormat::from_bytes(buffer);
            let mt = fmt.media_type();
            if mt == "application/octet-stream" {
                warn!("Could not guess mime type for {:?}", file_name);
            }
            file_format_from_content_type(&mt)
        }
        Err(e) => {
            warn!("Error guessing mime type for {:?}: {:?}", file_name, e);
            PsFileFormat::Bin
        }
    }
}