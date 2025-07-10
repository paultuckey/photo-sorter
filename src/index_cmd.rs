
use crate::util::{PsContainer, PsDirectoryContainer, PsZipContainer};
use anyhow::anyhow;
use log::{debug, info};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

pub(crate) async fn main(input: &String) -> anyhow::Result<()> {
    // todo: specify as glob patterns or regex?
    //   how to relate photos/videos to corresponding metadata?
    //   how to relate albums to corresponding photos/videos?
    //   do we care about edits to originals? yes ideally we would have one md with multiple
    //   what do we do about extra files that are not in the metadata? (eg, stuff that is in other users takeout but not ours)

    // strictest possible regex to identify a file

    // IC
    // albums
    // let ic_root = "Albums/*.csv";
    // let ic_root = "Memories/**/*.csv";
    //
    // // photos
    // let ic_root = "Photos/*.[HEIC|MOV|JPG|jpeg|MOV]";
    // let ic_root = "Recently Deleted/*.[HEIC|MOV|JPG|jpeg|MOV]";
    //
    // // index of files with meta
    // let ic_root = "Photos/Photo Details*.csv"; // may end with -1 -2 etc if there are many files
    //
    // // G
    // // people
    // let ic_root = "Google Photos/, */*.[HEIC|JPG|MOV]";
    //
    // // albums
    // let ic_root = "Google Photos/, */*metadata.json";
    //
    // // photos
    // let year_photos = "Google Photos/Photos from []/*.[HEIC|JPG|MOV]";
    // let ic_root = "Google Photos/Photos from */*.suppl.json";
    // let ic_root = "Google Photos/Photos from */*.supplemental-metadata.json";
    //
    // // regex to find year photos from google
    // let tags = vec![
    //     FileTag::new(
    //         "Google Photos year photo or video",
    //         r"Google Photos/Photos from (\d{4})/IMG_(\d+))\.(HEIC|JPG|JPEG|MOV)",
    //     ),
    //     FileTag::new(
    //         "Google Photos year supplemental metadata",
    //         r"Google Photos/Photos from (\d{4})/.suppl.json",
    //     ),
    // ];

    debug!("Inspecting: {input}");
    let path = Path::new(input);
    if !path.exists() {
        return Err(anyhow!("Input path does not exist: {}", input));
    }
    let mut container: Box<dyn PsContainer>;
    if path.is_dir() {
        info!("Input directory: {input}");
        container = Box::new(PsDirectoryContainer::new(input));
    } else {
        info!("Input zip: {input}");
        let tz = chrono::Local::now().offset().to_owned();
        container = Box::new(PsZipContainer::new(input, tz));
    }
    let idx = container.scan();

    let tags = create_file_tags();
    let mut scores: HashMap<String, i32> = HashMap::new();
    for si in idx {
        let matching_tags = find_matching_tags(&si.file_path, &tags);
        for matching_tag in matching_tags {
            if let Some(score) = scores.get_mut(&matching_tag.name) {
                *score += 1;
            } else {
                scores.insert(matching_tag.name.clone(), 1);
            }
        }
    }
    for (tag, count) in scores.iter() {
        info!("  {tag}: {count}");
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct FileTag {
    provider: String,
    name: String,
    regex: Regex,
}

impl FileTag {
    fn new(provider: &str, name: &str, pattern: &str) -> Result<FileTag, regex::Error> {
        Ok(FileTag {
            provider: provider.to_string(),
            name: name.to_string(),
            regex: Regex::new(pattern)?,
        })
    }

    fn matches(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }

    fn captures(&self, _input: &str) -> Option<regex::Captures> {
        //self.regex.clone().captures(input.clone())
        None
    }
}

fn create_file_tags() -> Vec<FileTag> {
    let patterns = vec![
        (
            "Google Photos",
            "year photo",
            r"Google Photos/Photos from (\d{4})/IMG_(\d+)\.(HEIC|JPG|JPEG|MOV)",
        ),
        (
            "Google Photos",
            "supplemental metadata",
            r"Google Photos/Photos from (\d{4})/.*\.json",
        ),
        ("iCloud Photos", "iCloud Photos", r"Photos/.*\.(HEIC|MOV|JPG|jpeg)"),
        ("iCloud Photos", "iCloud Albums", r"Albums/.*\.csv"),
        // Add your other 17 patterns here
    ];

    patterns
        .into_iter()
        .filter_map(|(provider, name, pattern)| FileTag::new(provider, name, pattern).ok())
        .collect::<Vec<FileTag>>()
}

fn find_matching_tags(file_path: &str, tags: &Vec<FileTag>) -> Vec<FileTag> {
    tags.iter()
        .filter(|tag| tag.matches(file_path))
        .map(|tag| tag.clone())
        .collect::<Vec<FileTag>>()
}
