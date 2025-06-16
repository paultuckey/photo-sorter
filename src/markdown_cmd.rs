use crate::media_file::{MediaFileInfo, media_file_info_from_path};
use crate::util::{checksum_file, checksum_string};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::debug;

pub fn main(input: &String, _: &Option<String>, dry_run: &bool) -> anyhow::Result<()> {
    println!("Inspecting: {}", input);
    let media_file_info = media_file_info_from_path(input);
    println!("Markdown:");
    println!();
    let mfm = mfm_from_media_file_info(&media_file_info);
    let s = assemble_markdown(&mfm, &"".to_string())?;
    println!("{}", s);
    Ok(())
}

fn mfm_from_media_file_info(media_file_info: &MediaFileInfo) -> MediaFrontMatter {
    let mut mfm = MediaFrontMatter {
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
    mfm: &MediaFrontMatter,
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
struct MediaFrontMatter {
    pub(crate) path: Option<String>,
    pub(crate) datetime_original: Option<String>,
    pub(crate) datetime: Option<String>,
    pub(crate) gps_date: Option<String>,
    pub(crate) unique_id: Option<String>,
}

fn parse_frontmatter(file_contents: &str) -> anyhow::Result<MediaFrontMatter> {
    let (fm, _) = split_frontmatter(file_contents)?;
    let mfm = parse_yaml(&fm)?;
    Ok(mfm)
}

/// We write yaml manually so we have _exact_ control over output.
/// We want to write plain style yaml, not the more complex
/// https://yaml.org/spec/1.2-old/spec.html#id2788859
fn generate_yaml(mfm: &MediaFrontMatter) -> anyhow::Result<String> {
    let mut yaml = String::new();
    yaml.push_str("photo-sorter:\n");
    if let Some(s) = mfm.path.clone() {
        yaml.push_str(&format!("  path: {}\n", s));
    }
    if let Some(s) = mfm.datetime_original.clone() {
        yaml.push_str(&format!("  original-datetime: {}\n", s));
    }
    if let Some(s) = mfm.datetime.clone() {
        yaml.push_str(&format!("  datetime: {}\n", s));
    }
    if let Some(s) = mfm.gps_date.clone() {
        yaml.push_str(&format!("  gps-date: {}\n", s));
    }
    Ok(yaml)
}

fn parse_yaml(s: &str) -> anyhow::Result<MediaFrontMatter> {
    let mfm: MediaFrontMatter = serde_yml::from_str(s)?;
    Ok(mfm)
}

fn split_frontmatter(file_contents: &str) -> anyhow::Result<(String, String)> {
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

fn assemble_markdown(mfm: &MediaFrontMatter, markdown_content: &String) -> anyhow::Result<String> {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&generate_yaml(mfm)?);
    s.push_str("---\n");
    s.push_str(markdown_content);
    Ok(s)
}

async fn file_exists(
    mfm: &MediaFrontMatter,
    checksum: &String,
    _: &String,
    base_dir: &String,
) -> anyhow::Result<()> {
    let path = mfm.path.clone().with_context(|| "Missing path")?;
    let file_path = Path::new(&base_dir).join(&path);
    let file_path = file_path.as_path();
    if file_path.exists() {
        let checksum_of_file = checksum_file(file_path)?;
        if checksum_of_file.eq(checksum) {
            debug!("File exists and checksums match");
            return Ok(());
        }
    }
    Ok(())
}
