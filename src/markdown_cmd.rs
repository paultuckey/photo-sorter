use crate::media::{MediaFileInfo, media_file_info_from_readable, get_desired_media_path};
use crate::util::{PsContainer, PsDirectoryContainer, checksum_file, checksum_string, checksum_bytes};
use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{debug, warn};

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
    let mut root = PsDirectoryContainer::new(parent_dir_string);
    let bytes = root
        .file_bytes(&file_name.to_string()) //
        .with_context(|| "Error reading media file")?;
    let media_file_info_res = media_file_info_from_readable(&bytes, input, &None);
    let Ok(media_file_info) = media_file_info_res else {
        debug!("Not a valid media file: {}", input);
        return Ok(());
    };
    debug!("Markdown:");
    let mfm = mfm_from_media_file_info(&media_file_info);
    let s = assemble_markdown(&mfm, "")?;
    println!("{}", s);
    Ok(())
}

pub(crate) fn mfm_from_media_file_info(media_file_info: &MediaFileInfo) -> PhotoSorterFrontMatter {
    let mut mfm = PhotoSorterFrontMatter {
        path: Some(media_file_info.original_path.clone()),
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
    let path = format!("{}.md", media_path);
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
    let mut existing_mfm_md = None;
    if output_c.exists(&output_path) {
        debug!("Markdown file already exists at {:?}", output_path);
        let existing_md_bytes = output_c.file_bytes(&output_path);
        let Ok(existing_md_bytes) = existing_md_bytes else {
            debug!("Could not read existing markdown file at {:?}", output_path);
            return Err(anyhow!("Could not read existing markdown file at {:?}", output_path));
        };
        let existing_md = String::from_utf8_lossy(&existing_md_bytes);
        let r = parse_frontmatter(&existing_md);
        let Ok((e_mfm, e_md)) = r else {
            warn!("Could not parse existing markdown file frontmatter at {:?}", output_path);
            return Err(anyhow!("Could not parse existing markdown file frontmatter at {:?}", output_path));
        };
        existing_mfm_md = Some((e_mfm, e_md));
    }

    let mfm = mfm_from_media_file_info(&media_file);
    let mut md = "".to_string();
    if let Some((e_mfm, e_md)) = existing_mfm_md {
        let y = generate_yaml(&mfm)?;
        let e_y = generate_yaml(&e_mfm)?;
        if y == e_y {
            debug!("Markdown file already exists with same frontmatter at {:?}", output_path);
            return Ok(());
        } else {
            // todo: seems to differ on path???
            debug!("Markdown file exists but frontmatter differs, copying markdown, clobbering frontmatter at {:?} {:?} {:?}", output_path, y, e_y);
            md = e_md;
        }
    }
    let s = assemble_markdown(&mfm, &md)?;
    let md_bytes = s.as_bytes().to_vec();
    output_c.write(dry_run, &output_path, &md_bytes);

    Ok(())
}

pub(crate) fn parse_frontmatter(file_contents: &str) -> anyhow::Result<(PhotoSorterFrontMatter, String)> {
    let (fm, md) = split_frontmatter(file_contents)?;
    let mfm = parse_yaml(&fm)?;
    Ok((mfm, md))
}

/// We write yaml manually so we have _exact_ control over output.
/// We want to write plain style yaml, not the more complex
/// https://yaml.org/spec/1.2-old/spec.html#id2788859
fn generate_yaml(mfm: &PhotoSorterFrontMatter) -> anyhow::Result<String> {
    let mut yaml = String::new();
    yaml.push_str("photo-sorter:\n");
    if let Some(s) = mfm.path.clone() {
        yaml.push_str(&format!("  path: {}\n", s));
    }
    if let Some(s) = mfm.datetime.clone() {
        yaml.push_str(&format!("  datetime: {}\n", s));
    }
    if let Some(s) = mfm.datetime_original.clone() {
        // If datetime and original datetime are the same, skip writing original datetime
        if s != mfm.datetime.clone().unwrap_or_default() {
            yaml.push_str(&format!("  original-datetime: {}\n", s));
        }
    }
    if let Some(s) = mfm.gps_date.clone() {
        yaml.push_str(&format!("  gps-date: {}\n", s));
    }
    Ok(yaml)
}

fn parse_yaml(s: &str) -> anyhow::Result<PhotoSorterFrontMatter> {
    let mfm: MediaFrontMatter = serde_yml::from_str(s)?;
    match mfm.photo_sorter {
        None => {
            Ok(PhotoSorterFrontMatter {
                path: None,
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

pub(crate) fn split_frontmatter(file_contents: &str) -> anyhow::Result<(String, String)> {
    let mut lines = file_contents.lines();
    let mut frontmatter = String::new();
    let mut content = String::new();

    // Check if file starts with frontmatter delimiter
    match lines.next() {
        Some("---") => {
            // Extract frontmatter until closing delimiter
            for line in &mut lines {
                if line == "---" {
                    break;
                }
                frontmatter.push_str(line);
                frontmatter.push('\n');
            }
        }
        Some(first_line) => {
            // No frontmatter, return empty frontmatter and original content
            content.push_str(first_line);
            content.push('\n');
        }
        None => {
            // Empty file
        }
    }

    // Remaining lines are content
    for line in lines {
        content.push_str(line);
        content.push('\n');
    }
    Ok((frontmatter, content))
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
async fn test_yaml() -> anyhow::Result<()> {
    crate::test_util::setup_log().await;
    let s = "---
photo-sorter:
  path: Google Photos/Photos from 2025/IMG_5071.HEIC
  datetime: 2025-02-09T18:17:01Z
  gps-date: 2025-02-09
---

Hello world";
    let r = parse_frontmatter(&s)?;
    assert_eq!(r.0, PhotoSorterFrontMatter {
                   path: Some("Google Photos/Photos from 2025/IMG_5071.HEIC".to_string()),
                   datetime_original: None,
                   datetime: Some("2025-02-09T18:17:01Z".to_string()),
                   gps_date: Some("2025-02-09".to_string()),
                   unique_id: None,
               });
    assert_eq!(r.1, "\nHello world\n".to_string());
    Ok(())
}

