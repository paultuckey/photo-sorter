use crate::util::{PsContainer, PsDirectoryContainer, PsZipContainer};
use anyhow::anyhow;
use log::{debug, info, warn};
use regex::{Regex};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;
use strum_macros::Display;

///
/// Do we understand all the dirs/files in a google takeout or icloud directory/zip?
/// Naming is pretty loose, especially in google takeout os this command is not overly useful.
/// It uses the strictest possible regex to identify dirs/files that match known patterns.
///
/// TODO:
///  - relate photos/videos to corresponding metadata
///  - how to relate albums to corresponding photos/videos
///  - relate edits/animations/originals together
///
pub(crate) async fn main(input: &String) -> anyhow::Result<()> {
    debug!("Inspecting: {input}");
    let path = Path::new(input);
    if !path.exists() {
        return Err(anyhow!("Input path does not exist: {}", input));
    }
    let container: Box<dyn PsContainer>;
    if path.is_dir() {
        info!("Input directory: {input}");
        container = Box::new(PsDirectoryContainer::new(input));
    } else {
        info!("Input zip: {input}");
        let tz = chrono::Local::now().offset().to_owned();
        container = Box::new(PsZipContainer::new(input, tz));
    }
    let idx = container.scan();

    let mut distinct_dirs: HashSet<String> = HashSet::new();
    for si in &idx {
        let p = Path::new(&si.file_path);
        if let Some(parent_path) = p.parent() {
            if let Some(pp_s) = parent_path.to_str() {
                if pp_s.is_empty() {
                    continue;
                }
                distinct_dirs.insert(pp_s.to_string());
            }
        }
    }

    let mut dir_matches: HashMap<String, i32> = HashMap::new();
    let mut unmatched_dirs = vec![];
    for dd in &distinct_dirs {
        let known_dirs = find_known_dirs(dd);
        if known_dirs.is_empty() {
            unmatched_dirs.push(dd);
        }
        for known_dir in known_dirs {
            if let Some(score) = dir_matches.get_mut(&known_dir.to_string()) {
                *score += 1;
            } else {
                dir_matches.insert(known_dir.to_string().clone(), 1);
            }
        }
    }

    let mut file_matches: HashMap<String, i32> = HashMap::new();
    let mut unmatched_files = vec![];
    for si in &idx {
        let known_files = find_known_files(&si.file_path);
        if known_files.is_empty() {
            unmatched_files.push(si);
        }
        for known_file in known_files {
            if let Some(score) = file_matches.get_mut(&known_file.to_string()) {
                *score += 1;
            } else {
                file_matches.insert(known_file.to_string().clone(), 1);
            }
        }
    }
    info!("Matched dirs:");
    for (known_dir, count) in dir_matches.iter() {
        info!("  {known_dir}: {count}");
    }
    info!("Unmatched dirs: {}", unmatched_dirs.len());
    for d in &unmatched_dirs {
        debug!("unmatches dir: {d}");
    }

    info!("Matched files:");
    for (known_file_type, count) in file_matches.iter() {
        info!("  {known_file_type}: {count}");
    }
    info!("Unmatched files: {}", unmatched_files.len());
    for si in &unmatched_files {
        debug!("unmatched file: {}", si.file_path);
    }
    Ok(())
}

#[derive(Debug, Clone, Display, PartialEq)]
enum KnownDir {
    // todo: do dirs change names for other languages? eg, es:fotos zh:照片?
    GtpNonAlbumForYear(String),
    GtpArchive,
    GtpBin,

    IcpNonAlbum,
    IcpAlbums,
    IcpMemories,
    IcpRecentlyDeleted,
}

#[derive(Debug, Clone, Display, PartialEq)]
enum KnownFileType {
    // todo: do file prefixes change for other languages?

    // can be in either provider
    Photo(String),
    Ignored, // any file we know it's file pattern but we don't need it

    // typically in google photos
    GtpMetadataJson(String),
    GtpPicasaSyncMetadataJson(String),
    GtpAlbumJson,
    PhotoWithGuid(String),
    GtpCollage(String),
    GtpAnimation(String),
    GtpPrintSubscription,
    GtpSharedAlbumComments,
    GtpUserGeneratedMemoryTitles,
    GtpArchiveBrowser,

    // typically in icloud photos
    IcpAlbumCsv(String),
    IcpSharedAlbumsZip,
}

fn match_re(haystack: &str, re: &Regex) -> Option<PatternMatch> {
    let haystack_lc = haystack.to_lowercase();
    //debug!("haystack: {haystack_lc} needle: {re}");
    let caps_o = re.captures(&haystack_lc);
    if let Some(caps) = caps_o {
        //debug!("Matched: {caps:?}");
        return Some(PatternMatch {
            g1: caps
                .get(1)
                .map_or("".to_string(), |m| m.as_str().to_string()),
        });
    }
    None
}

struct PatternMatch {
    g1: String,
}

fn make_file_patterns() -> Vec<(Vec<Regex>, MatchingFilePatternFn)> {
    let patterns: Vec<(&[&str], MatchingFilePatternFn)> = vec![
        (
            &[
                r"^img_([\d_]+)\.(heic|jpg|jpeg|mov|png)$",
                r"^([\d_]+)\.(heic|jpg|jpeg|mov|png)$",
                r"^img_([\d_]+)-edited\.(heic|jpg|jpeg|mov|png)$",
                r"^image_([\d_]+)\.(heic|jpg|jpeg|mov|png)$",
            ],
            |m| KnownFileType::Photo(m.g1),
        ),
        (
            &[
                r"^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.(heic|jpg|jpeg|mov|png)$",
                r"^([0-9]{11}__[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{9})\.(heic|jpg|jpeg|mov|png)$",
                r"^image_([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.(heic|jpg|jpeg|mov|png)$",
            ],
            |m| KnownFileType::PhotoWithGuid(m.g1),
        ),
        (
            &[
                r"^image_([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.(heic|jpg|jpeg|mov|png)\.json$",
                r"^(.+)\.(heic|jpg|jpeg|mov|png|gif)\.suppl\.json$",
                r"^(.+)\.(heic|jpg|jpeg|mov|png|gif)\.supplemental-meta\.json$",
                r"^(.+)\.(heic|jpg|jpeg|mov|png|gif)\.supplemental-metadata\([0-9]+\)\.json$",
                r"^(.+)\.(heic|jpg|jpeg|mov|png|gif)\.supplemental-metadata.json$",
            ],
            |m| KnownFileType::GtpMetadataJson(m.g1),
        ),
        (
            &[r"^picasasync\.supplemental-metadata\([0-9]+\).json$"],
            |m| KnownFileType::GtpPicasaSyncMetadataJson(m.g1),
        ),
        (&[r"^shared_album_comments.json$"], |_| {
            KnownFileType::GtpSharedAlbumComments
        }),
        (&[r"^archive_browser.html$"], |_| {
            KnownFileType::GtpArchiveBrowser
        }),
        (&[r"^user-generated-memory-titles.json$"], |_| {
            KnownFileType::GtpUserGeneratedMemoryTitles
        }),
        (
            &[r"^([\d_]+)-animation.gif$", r"^img_([\d_]+)-animation.gif$"],
            |m| KnownFileType::GtpAnimation(m.g1),
        ),
        (&[r"^([\d_]+)-collage.jpg$"], |m| {
            KnownFileType::GtpCollage(m.g1)
        }),
        (&[r"^print-subscriptions.json$"], |_| {
            KnownFileType::GtpPrintSubscription
        }),
        (&[r"^metadata.json$"], |_| KnownFileType::GtpAlbumJson),
        (&[r"^(.+)\.csv$"], |m| KnownFileType::IcpAlbumCsv(m.g1)),
        (&[r"^icloud shared albums.zip$"], |_| {
            KnownFileType::IcpSharedAlbumsZip
        }),
        (&[r"^\.ds_store$"], |_| KnownFileType::Ignored),
    ];
    patterns
        .iter()
        .filter_map(|(patterns, match_fn)| {
            let mut regexes: Vec<Regex> = vec![];
            for p in patterns.iter() {
                match Regex::new(p) {
                    Ok(re) => regexes.push(re),
                    Err(re_err) => {
                        warn!("Error while parsing: {re_err}");
                    }
                }
            }
            Some((regexes, *match_fn))
        })
        .collect::<Vec<(Vec<Regex>, MatchingFilePatternFn)>>()
}

fn make_dir_patterns() -> Vec<(Vec<Regex>, MatchingDirPatternFn)> {
    let patterns: Vec<(&[&str], MatchingDirPatternFn)> = vec![
        (&[r"^google photos/photos from (\d{4})$"], |m| {
            KnownDir::GtpNonAlbumForYear(m.g1)
        }),
        (&[r"^photos$"], |_| KnownDir::IcpNonAlbum),
        (&[r"^albums$"], |_| KnownDir::IcpAlbums),
        (&[r"^memories$"], |_| KnownDir::IcpMemories),
        (&[r"^archive"], |_| KnownDir::GtpArchive),
        (&[r"^bin"], |_| KnownDir::GtpBin),
        (&[r"^memories/(.+)$"], |_| KnownDir::IcpMemories),
        (&[r"^recently deleted"], |_| KnownDir::IcpRecentlyDeleted),
    ];
    patterns
        .iter()
        .filter_map(|(patterns, match_fn)| {
            let mut regexes: Vec<Regex> = vec![];
            for p in patterns.iter() {
                match Regex::new(p) {
                    Ok(re) => regexes.push(re),
                    Err(re_err) => {
                        warn!("Error while parsing: {re_err}");
                    }
                }
            }
            Some((regexes, *match_fn))
        })
        .collect::<Vec<(Vec<Regex>, MatchingDirPatternFn)>>()
}

type MatchingFilePatternFn = fn(PatternMatch) -> KnownFileType;
type MatchingDirPatternFn = fn(PatternMatch) -> KnownDir;

static FILE_PATTERNS: LazyLock<Vec<(Vec<Regex>, MatchingFilePatternFn)>> =
    LazyLock::new(make_file_patterns);
static DIR_PATTERNS: LazyLock<Vec<(Vec<Regex>, MatchingDirPatternFn)>> =
    LazyLock::new(make_dir_patterns);

fn find_known_files(file_path: &str) -> Vec<KnownFileType> {
    let p = Path::new(file_path);
    match p.file_name() {
        None => {
            vec![]
        }
        Some(file_name) => match file_name.to_str() {
            None => {
                vec![]
            }
            Some(fn2) => {
                let known_files = FILE_PATTERNS
                    .iter()
                    .flat_map(|(patterns, match_fn)| {
                        let mut matches = vec![];
                        for p in patterns.iter() {
                            if let Some(matched) = match_re(fn2, p) {
                                matches.push(match_fn(matched))
                            }
                        }
                        matches
                    })
                    .collect::<Vec<KnownFileType>>();
                if known_files.len() > 1 {
                    warn!(
                        "File {fn2} had {} matches, this indicated overlapping regexes",
                        known_files.len()
                    )
                }
                known_files
            }
        },
    }
}

fn find_known_dirs(dir_path: &str) -> Vec<KnownDir> {
    let known_dirs = DIR_PATTERNS
        .iter()
        .flat_map(|(patterns, match_fn)| {
            let mut matches = vec![];
            for p in patterns.iter() {
                if let Some(matched) = match_re(dir_path, p) {
                    matches.push(match_fn(matched))
                }
            }
            matches
        })
        .collect::<Vec<KnownDir>>();
    if known_dirs.len() > 1 {
        warn!(
            "File {dir_path} had {} matches, this indicated overlapping regexes",
            known_dirs.len()
        )
    }
    known_dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test()]
    async fn test_no_match() {
        crate::test_util::setup_log().await;
        assert_eq!(find_known_files("/hello"), vec![]);
        assert_eq!(
            find_known_files("Google Photos/Photos from 2012/IMG_1234.jpg"),
            vec![KnownFileType::Photo(String::from("1234"))]
        );
        assert_eq!(
            find_known_files("Google Photos/2016-book/IMG_1316.JPG.supplemental-metadata.json"),
            vec![KnownFileType::GtpMetadataJson(String::from("img_1316"))]
        );
    }

    #[tokio::test()]
    async fn test_enum_to_string() {
        crate::test_util::setup_log().await;
        assert_eq!(
            KnownFileType::IcpAlbumCsv("something".to_string()).to_string(),
            "IcpAlbumCsv".to_string()
        );
    }
}
