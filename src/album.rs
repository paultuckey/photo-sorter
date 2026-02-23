use crate::file_type::{AccurateFileType, QuickFileType};
use crate::fs::FileSystem;
use crate::media::MediaFileInfo;
use crate::util::{ScanInfo, dir_part, name_part};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

pub(crate) fn parse_album(
    container: &dyn FileSystem,
    si: &ScanInfo,
    si_files: &[ScanInfo],
) -> Option<Album> {
    match si.quick_file_type {
        QuickFileType::AlbumCsv => parse_csv_album(container, si),
        QuickFileType::AlbumJson => parse_json_album(container, si, si_files),
        _ => None,
    }
}

fn parse_csv_album(container: &dyn FileSystem, si: &ScanInfo) -> Option<Album> {
    info!("Parse CSV album: {:?}", &si.file_path);
    let reader_r = container.open(&si.file_path);
    let Ok(reader) = reader_r else {
        warn!("No bytes for album: {:?}", &si.file_path);
        return None;
    };
    let name = &si.file_path;
    let mut rdr = csv::Reader::from_reader(reader);
    let Ok(s) = rdr.headers() else {
        debug!("  No headers");
        return None;
    };
    if s.is_empty() {
        debug!("  Headers empty");
        return None;
    }
    let Some(col0) = s.get(0) else {
        debug!("  No first header");
        return None;
    };
    if col0.trim().to_lowercase() != "Images".to_lowercase() {
        debug!("  Not an iCloud album (column 0 should be 'Images', was {col0})");
        return None;
    }
    let mut files: Vec<String> = vec![];

    for result in rdr.records() {
        let Ok(record) = result else {
            debug!("Error reading record");
            continue;
        };
        debug!("{record:?}");
        if record.is_empty() {
            continue;
        }
        let Some(file_name) = record.get(0) else {
            continue;
        };

        // look for file with the original path {} + file_name
        let directory_path_str = dir_part(&si.file_path);

        let original_file = Path::new(&directory_path_str)
            .join(file_name)
            .to_string_lossy()
            .to_string();

        files.push(original_file);
    }
    if files.is_empty() {
        debug!("Not an album: {name:?}");
        return None;
    }
    // find index of last dot and get all chars before that
    let name_without_ext = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.clone());

    if name_without_ext.is_empty() {
        debug!("Album file has no name: {name:?}");
        return None;
    }
    info!(
        "Found album: {:?} with {:?} entries at {:?}",
        name_without_ext,
        files.len(),
        name
    );
    Some(Album {
        title: name_without_ext.clone(),
        desired_album_md_path: name.clone(),
        files,
    })
}

fn parse_json_album(
    container: &dyn FileSystem,
    si: &ScanInfo,
    all_scanned_files: &[ScanInfo],
) -> Option<Album> {
    let reader_r = container.open(&si.file_path);
    let Ok(reader) = reader_r else {
        warn!("No bytes for album: {:?}", &si.file_path);
        return None;
    };
    let j: Result<Value, _> = serde_json::from_reader(reader);
    let title;
    if let Ok(j) = j {
        let title_res = j.get("title");
        if let Some(title_value) = title_res {
            debug!("  Found album title: {title_value}");
            title = Some(title_value.as_str().unwrap_or("").to_string());
        } else {
            warn!("Title not found in JSON, skipping {:?}", &si.file_path);
            return None;
        }
    } else {
        warn!("Unable to decode album JSON: {:?}", &si.file_path);
        return None;
    }
    // all files in this directory are in the album
    let directory_path_str = dir_part(&si.file_path);
    // look up the media path in the media_path_map
    let same_dir_files = all_scanned_files
        .iter()
        .filter(|si| {
            let q_dir_part = &dir_part(&si.file_path);
            si.quick_file_type == QuickFileType::Media && directory_path_str.eq(q_dir_part)
        })
        .map(|si| si.file_path.clone())
        .collect::<Vec<String>>();

    let directory_path_name_str = name_part(&directory_path_str);
    let desired_album_md_path = format!("albums/{directory_path_name_str}.md");
    // todo: how check for existing album?
    Some(Album {
        desired_album_md_path,
        title: title.unwrap_or(directory_path_name_str),
        files: same_dir_files,
    })
}

pub(crate) struct Album {
    pub(crate) desired_album_md_path: String,
    pub(crate) title: String,
    files: Vec<String>,
}

pub(crate) fn build_album_md(
    album: &Album,
    all_media_o: Option<&HashMap<String, MediaFileInfo>>,
    media_relative_path: &str,
    final_path_by_checksum: Option<&HashMap<String, String>>,
) -> String {
    let mut md = String::new();
    let generated_warning =
        "\n\n\n[ This file is a GENERATED album, do NOT edit it directly ]: #\n\n\n";
    // todo: in yaml front matter for media link back to album
    md.push_str(generated_warning);
    md.push_str(&format!("# {}", &album.title));
    md.push_str("\n\n");
    for f in album.files.clone() {
        let target_path_o: Option<String>;
        if let Some(all_media) = all_media_o {
            target_path_o = all_media
                .values()
                .find(|m| {
                    m.accurate_file_type != AccurateFileType::Unsupported
                        && m.quick_file_type == QuickFileType::Media
                        && m.original_path.iter().any(|p| p.eq(&f))
                })
                .and_then(|m| {
                    let long_checksum = &m.hash_info.long_checksum;
                    final_path_by_checksum.and_then(|fp_map| fp_map.get(long_checksum).cloned())
                });
            if target_path_o.is_none() {
                warn!("No media file desired path found for: {f}");
                continue;
            }
        } else {
            // intentionally use the original path
            target_path_o = Some(f.clone());
        }
        if let Some(target_path) = target_path_o.clone() {
            let alt_text = "Photo";
            let path = format!("{media_relative_path}{target_path}");
            md.push_str(&format!("\n![{alt_text}]({path})"));
        } else {
            warn!("Target path empty: {f}");
        }
    }
    md.push_str("\n\n");
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::OsFileSystem;

    #[test]
    fn test_ic_sample() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let c = OsFileSystem::new(&"test".to_string());
        let qsf = ScanInfo::new("ic-album-sample.csv".to_string(), None, None, 0);
        let a = parse_album(&c, &qsf, &vec![]).unwrap();
        assert_eq!(a.title, "ic-album-sample".to_string());
        assert_eq!(a.files.len(), 5);
        assert_eq!(
            a.files.get(0).unwrap(),
            "35F8739B-30E0-4620-802C-0817AD7356F6.JPG"
        );
        Ok(())
    }

    #[test]
    fn test_g_sample() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let c = OsFileSystem::new(&"test/takeout1".to_string());
        let qsf = ScanInfo::new("Google Photos/album1/metadata.json".to_string(), None, None, 0);
        let si1 = ScanInfo::new("Google Photos/album1/test1.jpg".to_string(), None, None, 0);
        let si2 = ScanInfo::new("different/test2.jpg".to_string(), None, None, 0);
        let a = parse_album(&c, &qsf, &vec![si1, si2]).unwrap();
        assert_eq!(a.title, "Some album title".to_string());
        assert_eq!(a.files.len(), 1);
        assert_eq!(
            a.files.get(0).unwrap().to_string(),
            "Google Photos/album1/test1.jpg".to_string()
        );
        Ok(())
    }

    #[test]
    fn test_build_album_md_no_media_info() {
        let album = Album {
            desired_album_md_path: "albums/test.md".to_string(),
            title: "Test Album".to_string(),
            files: vec!["file1.jpg".to_string(), "file2.jpg".to_string()],
        };
        let md = build_album_md(&album, None, "../media/", None);
        assert!(md.contains("# Test Album"));
        assert!(md.contains("![Photo](../media/file1.jpg)"));
        assert!(md.contains("![Photo](../media/file2.jpg)"));
    }

    #[test]
    fn test_build_album_md_with_mappings() {
        let album = Album {
            desired_album_md_path: "albums/test.md".to_string(),
            title: "Test Album".to_string(),
            files: vec!["file1.jpg".to_string()],
        };
        let mut media_info = MediaFileInfo::new_for_test();
        media_info.original_path = vec!["file1.jpg".to_string()];
        media_info.hash_info.long_checksum = "longhash1".to_string();

        let mut all_media = HashMap::new();
        all_media.insert("key1".to_string(), media_info);

        let mut final_path_by_checksum = HashMap::new();
        final_path_by_checksum.insert("longhash1".to_string(), "2023/01/file1.jpg".to_string());

        let md = build_album_md(&album, Some(&all_media), "../media/", Some(&final_path_by_checksum));
        assert!(md.contains("# Test Album"));
        assert!(md.contains("![Photo](../media/2023/01/file1.jpg)"));
    }

    #[test]
    fn test_build_album_md_missing_mapping() {
        let album = Album {
            desired_album_md_path: "albums/test.md".to_string(),
            title: "Test Album".to_string(),
            files: vec!["file1.jpg".to_string()],
        };
        let all_media = HashMap::new(); // Empty
        let final_path_by_checksum = HashMap::new();

        let md = build_album_md(&album, Some(&all_media), "../media/", Some(&final_path_by_checksum));
        assert!(md.contains("# Test Album"));
        assert!(!md.contains("![Photo]")); // Should be skipped
    }

    #[test]
    fn test_build_album_md_missing_final_path() {
        let album = Album {
            desired_album_md_path: "albums/test.md".to_string(),
            title: "Test Album".to_string(),
            files: vec!["file1.jpg".to_string()],
        };
        let mut media_info = MediaFileInfo::new_for_test();
        media_info.original_path = vec!["file1.jpg".to_string()];
        media_info.hash_info.long_checksum = "longhash1".to_string();

        let mut all_media = HashMap::new();
        all_media.insert("key1".to_string(), media_info);

        let final_path_by_checksum = HashMap::new(); // Empty, so lookup fails

        let md = build_album_md(&album, Some(&all_media), "../media/", Some(&final_path_by_checksum));
        assert!(md.contains("# Test Album"));
        assert!(!md.contains("![Photo]")); // Should be skipped
    }
}
