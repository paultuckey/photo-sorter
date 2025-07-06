use crate::media::{MediaFileInfo, media_file_info_from_readable};
use crate::util::{PsContainer, PsDirectoryContainer, checksum_file, checksum_string, checksum_bytes};
use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use log::{debug, warn};
use crate::file_type::{quick_scan_file};

pub fn main(input: &String) -> anyhow::Result<()> {
    debug!("Inspecting: {}", input);
    let p = Path::new(input);
    let parent_dir = p
        .parent() //
        .with_context(|| "Unable to get parent directory")?;
    let parent_dir_string = parent_dir.to_string_lossy().to_string();
    let file_name = p
        .file_name()
        .with_context(|| "Unable to get file name")?
        .to_string_lossy();
    let mut root: Box<dyn PsContainer> = Box::new(PsDirectoryContainer::new(parent_dir_string));
    let qsf_o = quick_scan_file(&root, input);
    let Some(qsf) = qsf_o else {
        debug!("Not a valid media file: {}", input);
        return Ok(());
    };
    let bytes = root
        .file_bytes(&file_name.to_string()) //
        .with_context(|| "Error reading media file")?;
    let checksum_o = checksum_bytes(&bytes).ok();
    let Some((short_checksum, long_checksum)) = checksum_o else {
        debug!("Could not calculate checksum for file: {:?}", qsf.name);
        return Err(anyhow!("Could not calculate checksum for file: {:?}", qsf.name));
    };

    // todo: extra info
    let media_file_info_res = media_file_info_from_readable(
        &qsf, &bytes, &None, &short_checksum, &long_checksum);
    let Ok(media_file_info) = media_file_info_res else {
        debug!("Not a valid media file: {}", input);
        return Ok(());
    };
    debug!("Markdown:");
    let mfm = mfm_from_media_file_info(&media_file_info);
    let s = assemble_markdown(&mfm, "")?;
    println!("{s}");
    Ok(())
}

pub(crate) fn mfm_from_media_file_info(media_file_info: &MediaFileInfo) -> PhotoSorterFrontMatter {
    let mut mfm = PhotoSorterFrontMatter {
        path: media_file_info.desired_media_path.clone(),
        path_original: media_file_info.original_path.clone(),
        datetime_original: None,
        datetime: None,
        gps_date: None,
        unique_id: None,
    };
    if let Some(exif) = media_file_info.parsed_exif.clone() {
        if let Some(dt) = exif.datetime_original {
            mfm.datetime_original = Some(dt);
        }
        if let Some(dt) = exif.datetime {
            mfm.datetime = Some(dt);
        }
        if let Some(gps_date) = exif.gps_date {
            mfm.gps_date = Some(gps_date);
        }
        if let Some(unique_id) = exif.unique_id {
            mfm.unique_id = Some(unique_id);
        }
    }
    mfm
}

async fn save_markdown(
    mfm: &PhotoSorterFrontMatter,
    markdown: &String,
    base_dir: &String,
) -> anyhow::Result<()> {
    let media_path = mfm.path.clone().with_context(|| "Missing path")?;
    let path = format!("{media_path}.md");
    let file_path = Path::new(&base_dir).join(&path);
    let file_path = file_path.as_path();
    if file_path.exists() {
        debug!("md file exists");
        let checksum_of_string = checksum_string(markdown)?;
        let checksum_of_file = checksum_file(Path::new(&file_path))?;
        if checksum_of_file == checksum_of_string {
            debug!("checksums match");
            return Ok(());
        }
    }

    let prefix = file_path.parent().with_context(|| "No parent")?;
    fs::create_dir_all(prefix).with_context(|| "Unable to create dirs")?;

    debug!("writing md");
    // let mut file = fs::File::create(file_path) //
    //     .with_context(|| format!("Unable to create file: {:?}", file_path))?;
    // file.write_all(markdown.as_bytes()) //
    //     .with_context(|| format!("Unable to write to file: {:?}", file_path))?;
    Ok(())
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all(deserialize = "kebab-case", serialize = "kebab-case"))]
pub(crate) struct PhotoSorterFrontMatter {
    pub(crate) path: Option<String>,
    pub(crate) path_original: Vec<String>,
    pub(crate) datetime_original: Option<String>,
    pub(crate) datetime: Option<String>,
    pub(crate) gps_date: Option<String>,
    pub(crate) unique_id: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all(deserialize = "kebab-case", serialize = "kebab-case"))]
pub(crate) struct MediaFrontMatter {
    pub(crate) photo_sorter: Option<PhotoSorterFrontMatter>,
}

pub(crate) fn sync_markdown(dry_run: bool, media_file: &MediaFileInfo, output_c: &mut PsDirectoryContainer) -> anyhow::Result<()> {
    let Some(output_path) = media_file.desired_markdown_path.clone() else {
        debug!("No desired markdown path for media file: {:?}", media_file.original_path);
        return Ok(());
    };
    let mfm = mfm_from_media_file_info(media_file);
    let yaml = generate_yaml(&mfm)?;
    let mut md = "".to_string();

    if output_c.exists(&output_path) {
        let existing_md_bytes_r = output_c.file_bytes(&output_path);
        let Ok(existing_md_bytes) = existing_md_bytes_r else {
            debug!("Could not read existing markdown file at {:?}", output_path);
            return Err(anyhow!("Could not read existing markdown file at {:?}", output_path));
        };
        let existing_full_md = String::from_utf8_lossy(&existing_md_bytes);
        let (e_mfm_o, e_md) = parse_frontmatter(&existing_full_md, &output_path);

        if let Some(e_mfm) = e_mfm_o {
            let e_yaml = generate_yaml(&e_mfm)?;
            if yaml == e_yaml {
                debug!("Markdown file already exists with same frontmatter at {:?}", output_path);
                return Ok(());
            } else {
                debug!("Markdown file exists but frontmatter differs, copying markdown, clobbering frontmatter at {:?} {:?} {:?}", output_path, yaml, e_yaml);
                md = e_md;
            }
        } else {
            // frontmatter is empty, we will write new frontmatter but copy markdown content
            debug!("Markdown file already exists with empty frontmatter at {:?}", output_path);
            md = e_md;
        }
    }

    let md_str = assemble_markdown(&mfm, &md)?;
    let md_bytes = md_str.as_bytes().to_vec();
    output_c.write(dry_run, &output_path, &md_bytes);
    Ok(())
}

pub(crate) fn parse_frontmatter(file_contents: &str, path: &str) -> (Option<PhotoSorterFrontMatter>, String) {
    let (fm, md) = split_frontmatter(file_contents);
    let mfm_r = parse_yaml(&fm);
    match mfm_r {
        Ok(mfm) => {
            (Some(mfm), md)
        }
        Err(_) => {
            warn!("Could not parse frontmatter at {:?}, treating as empty", path);
            (None, md)
        }
    }
}

/// We write yaml manually so we have _exact_ control over output.
/// We want to write plain style yaml, not the more complex
/// https://yaml.org/spec/1.2-old/spec.html#id2788859
fn generate_yaml(mfm: &PhotoSorterFrontMatter) -> anyhow::Result<String> {
    let mut yaml = String::new();
    yaml.push_str("photo-sorter:\n");
    if let Some(s) = mfm.path.clone() {
        yaml.push_str(&format!("  path: {s}\n"));
    }
    if !mfm.path_original.is_empty() {
        yaml.push_str("  path-original:\n");
        for po in mfm.path_original.clone() {
            yaml.push_str(&format!("    - {po}\n"));
        }
    }
    if let Some(s) = mfm.datetime.clone() {
        yaml.push_str(&format!("  datetime: {s}\n"));
    }
    if let Some(s) = mfm.datetime_original.clone() {
        // If datetime and original datetime are the same, skip writing original datetime
        if s != mfm.datetime.clone().unwrap_or_default() {
            yaml.push_str(&format!("  original-datetime: {s}\n"));
        }
    }
    if let Some(s) = mfm.gps_date.clone() {
        yaml.push_str(&format!("  gps-date: {s}\n"));
    }
    Ok(yaml)
}

fn parse_yaml(s: &str) -> anyhow::Result<PhotoSorterFrontMatter> {
    let mfm: MediaFrontMatter = serde_yml::from_str(s)?;
    match mfm.photo_sorter {
        None => {
            Ok(PhotoSorterFrontMatter {
                path: None,
                path_original: vec![],
                datetime_original: None,
                datetime: None,
                gps_date: None,
                unique_id: None,
            })
        }
        Some(psfm) => {
            Ok(psfm)
        }
    }
}

/// Grab anything between "---[\r]\n" and "---[\r]\n" and put into .0. Put everything else into .1.
/// If any sort of invalid case is encountered, return empty frontmatter and original content.
pub(crate) fn split_frontmatter(file_contents: &str) -> (String, String) {
    // Handle leading whitespace - trim leading newlines and carriage returns
    let trimmed = file_contents.trim_start_matches(['\n', '\r']);

    // Check if the file starts with "---"
    if !trimmed.starts_with("---") {
        return ("".to_string(), file_contents.to_string());
    }

    // Find the first newline after the opening "---"
    let (line_ending, after_first_delim) = if trimmed.starts_with("---\r\n") {
        ("\r\n", &trimmed[5..]) // Skip "---\r\n"
    } else if trimmed.starts_with("---\n") {
        ("\n", &trimmed[4..]) // Skip "---\n"
    } else {
        // No newline after opening "---", treat as invalid
        return ("".to_string(), file_contents.to_string());
    };

    // Find the closing "---" delimiter
    if let Some(end_pos) = after_first_delim.find("---") {
        let potential_frontmatter = &after_first_delim[..end_pos];
        let after_end_delim = &after_first_delim[end_pos..];

        // Check if the closing "---" is followed by a newline or is at the end
        if after_end_delim.starts_with("---\r\n") {
            let remaining_content = &after_end_delim[5..]; // Skip "---\r\n"

            // Special case: if frontmatter is empty, return original content
            if potential_frontmatter.trim().is_empty() {
                return ("".to_string(), file_contents.to_string());
            }

            // Remove trailing newline from frontmatter if present
            let fm = potential_frontmatter.trim_end_matches(['\n', '\r']).to_string();
            // If remaining content is empty, but we had a newline after ---, include it
            if remaining_content.is_empty() {
                return (fm, "\r\n".to_string());
            } else {
                return (fm, remaining_content.to_string());
            }
        } else if after_end_delim.starts_with("---\n") {
            let remaining_content = &after_end_delim[4..]; // Skip "---\n"

            // Special case: if frontmatter is empty, return original content
            if potential_frontmatter.trim().is_empty() {
                return ("".to_string(), file_contents.to_string());
            }

            // Remove trailing newline from frontmatter if present
            let fm = potential_frontmatter.trim_end_matches(['\n', '\r']).to_string();
            // If remaining content is empty, but we had a newline after ---, include it
            if remaining_content.is_empty() {
                return (fm, "\n".to_string());
            } else {
                return (fm, remaining_content.to_string());
            }
        } else if after_end_delim.starts_with("---") {
            // Check what comes after the closing "---"
            let after_closing = &after_end_delim[3..];

            // Special case: if frontmatter is empty, return original content
            if potential_frontmatter.trim().is_empty() {
                return ("".to_string(), file_contents.to_string());
            }

            // Remove trailing newline from frontmatter if present
            let fm = potential_frontmatter.trim_end_matches(['\n', '\r']).to_string();

            // If there's content after the closing ---, it should be the remaining content
            // If the original had CRLF line endings, preserve that in the remaining content
            if !after_closing.is_empty() {
                let remaining_with_newline = format!("{line_ending}{after_closing}");
                return (fm, remaining_with_newline);
            } else {
                // File ends with "---"
                return (fm, "".to_string());
            }
        }
    }

    // No valid closing delimiter found, treat as invalid
    ("".to_string(), file_contents.to_string())
}

pub(crate) fn assemble_markdown(
    mfm: &PhotoSorterFrontMatter,
    markdown_content: &str,
) -> anyhow::Result<String> {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&generate_yaml(mfm)?);
    s.push_str("---\n");
    s.push_str(markdown_content);
    Ok(s)
}

async fn file_exists(
    mfm: &PhotoSorterFrontMatter,
    long_checksum: &String,
    _: &String,
    base_dir: &String,
) -> anyhow::Result<()> {
    let path = mfm.path.clone().with_context(|| "Missing path")?;
    let file_path = Path::new(&base_dir).join(&path);
    let file_path = file_path.as_path();
    if file_path.exists() {
        let (_, checksum_of_file) = checksum_file(file_path)?;
        if checksum_of_file.eq(long_checksum) {
            debug!("File exists and checksums match");
            return Ok(());
        }
    }
    Ok(())
}

#[tokio::test()]
async fn test_parse_frontmatter() {
    crate::test_util::setup_log().await;
    let (fm_o, md) = parse_frontmatter("---
  photo-sorter:
    path: 2025/02/09/1123-23-abcdefg.jpg
    path-original:
      - Google Photos/Photos from 2025/IMG_5071.HEIC
    datetime: 2025-02-09T18:17:01Z
    gps-date: 2025-02-09
---
x
last line", "test.md");
    assert_eq!(fm_o.unwrap(), PhotoSorterFrontMatter {
        path: Some("2025/02/09/1123-23-abcdefg.jpg".to_string()),
        path_original: vec!["Google Photos/Photos from 2025/IMG_5071.HEIC".to_string()],
        datetime_original: None,
        datetime: Some("2025-02-09T18:17:01Z".to_string()),
        gps_date: Some("2025-02-09".to_string()),
        unique_id: None,
    });
    assert_eq!(md, "x\nlast line".to_string());
}

#[test]
fn parse_with_missing_beginning_line() {
    let text = "";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "");
    assert_eq!(md, "");
}

#[test]
fn parse_with_missing_ending_line() {
    let text = "---\n";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "");
    assert_eq!(md, "---\n");
}

#[test]
fn parse_with_missing_ending_line_crlf() {
    let text = "---\r\n";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "");
    assert_eq!(md, "---\r\n");
}

#[test]
fn parse_with_empty_frontmatter() {
    let text = "---\n---\n";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "");
    assert_eq!(md, "---\n---\n");
}

#[test]
fn parse_with_empty_frontmatter_crlf() {
    let text = "---\r\n---\r\n";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "");
    assert_eq!(md, "---\r\n---\r\n");
}

#[test]
fn parse_with_missing_known_field() {
    let text = "---\ndate: 2000-01-01\n---\n";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "date: 2000-01-01");
    assert_eq!(md, "\n");
}

#[test]
fn parse_with_missing_known_field_crlf() {
    let text = "---\r\ndate: 2000-01-01\r\n---\r\n";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "date: 2000-01-01");
    assert_eq!(md, "\r\n");
}

#[test]
fn parse_with_valid_frontmatter() {
    let text = "---\ntitle: dummy_title---\ndummy_body";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "title: dummy_title");
    assert_eq!(md, "dummy_body");
}

#[test]
fn parse_with_valid_frontmatter_crlf() {
    let text = "---\r\ntitle: dummy_title---\r\ndummy_body";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "title: dummy_title");
    assert_eq!(md, "dummy_body");
}

#[test]
fn parse_with_extra_whitespace() {
    let text = "\n\n\n---\ntitle: dummy_title---\ndummy_body";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "title: dummy_title");
    assert_eq!(md, "dummy_body");
}

#[test]
fn parse_md_only_with_no_frontmatter() {
    let text = "\n\n\ndummy_body";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "");
    assert_eq!(md, "\n\n\ndummy_body");
}

#[test]
fn parse_with_extra_whitespace_rn() {
    let text = "\r\n\r\n\r\n---\r\ntitle: dummy_title---\r\ndummy_body";
    let (fm, md) = split_frontmatter(text);
    assert_eq!(fm, "title: dummy_title");
    assert_eq!(md, "dummy_body");
}
