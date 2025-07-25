use anyhow::{anyhow};
use log::{debug, warn};
use crate::media::MediaFileInfo;
use crate::util::{PsContainer, PsDirectoryContainer};
use yaml_rust2::{YamlLoader, YamlEmitter, Yaml};
use yaml_rust2::yaml::Hash;

pub(crate) fn mfm_from_media_file_info(media_file_info: &MediaFileInfo) -> PhotoSorterFrontMatter {
    let mut mfm = PhotoSorterFrontMatter {
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

pub(crate) struct PhotoSorterFrontMatter {
    pub(crate) path_original: Vec<String>,
    pub(crate) datetime_original: Option<String>,
    pub(crate) datetime: Option<String>,
    pub(crate) gps_date: Option<String>,
    pub(crate) unique_id: Option<String>,
    // todo: add supplemental fields?
}

pub(crate) fn sync_markdown(dry_run: bool, media_file: &MediaFileInfo, output_c: &mut PsDirectoryContainer) -> anyhow::Result<()> {
    let Some(output_path) = media_file.desired_markdown_path.clone() else {
        warn!("No desired markdown path for media file: {:?}", media_file.original_path);
        return Ok(());
    };
    let mfm = mfm_from_media_file_info(media_file);
    let mut e_md = "".to_string();
    let mut e_yaml = None;

    if output_c.exists(&output_path) {
        let existing_md_bytes_r = output_c.file_bytes(&output_path);
        let Ok(existing_md_bytes) = existing_md_bytes_r else {
            warn!("Could not read existing markdown file at {output_path:?}");
            return Err(anyhow!("Could not read existing markdown file at {output_path:?}"));
        };
        let existing_full_md = String::from_utf8_lossy(&existing_md_bytes);
        let (e_yaml_i, e_md_i) = split_frontmatter(&existing_full_md);
        e_yaml = Some(e_yaml_i);
        e_md = e_md_i;
    }
    let md_str = assemble_markdown(&mfm, &e_yaml, &e_md)?;
    let md_bytes = md_str.as_bytes().to_vec();
    output_c.write(dry_run, &output_path, &md_bytes);
    Ok(())
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
    let (line_ending, after_first_delim) = if let Some(stripped) = trimmed.strip_prefix("---\r\n") {
        ("\r\n", stripped) // Skip "---\r\n"
    } else if let Some(stripped) = trimmed.strip_prefix("---\n") {
        ("\n", stripped) // Skip "---\n"
    } else {
        // No newline after opening "---", treat as invalid
        return ("".to_string(), file_contents.to_string());
    };

    // Find the closing "---" delimiter
    if let Some(end_pos) = after_first_delim.find("---") {
        let potential_frontmatter = &after_first_delim[..end_pos];
        let after_end_delim = &after_first_delim[end_pos..];

        // Check if the closing "---" is followed by a newline or is at the end
        if let Some(remaining_content) = after_end_delim.strip_prefix("---\r\n") {

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
        } else if let Some(remaining_content) = after_end_delim.strip_prefix("---\n") {

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
        } else if let Some(after_closing) = after_end_delim.strip_prefix("---") {

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
    existing_yaml: &Option<String>,
    markdown_content: &str,
) -> anyhow::Result<String> {
    let new_yaml = merge_yaml(existing_yaml, mfm);
    if new_yaml.is_empty() {
        warn!("Generated YAML is empty, returning markdown content");
        return Ok(markdown_content.to_string());
    }
    if let Some(existing_yaml) = existing_yaml {
        if new_yaml.eq(existing_yaml) {
            warn!("Generated YAML matches existing, returning original content");
            // todo: better return type
            return Ok(markdown_content.to_string());
        }
    }
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&new_yaml);
    s.push_str("---\n");
    s.push_str(markdown_content);
    Ok(s)
}

fn merge_yaml(s: &Option<String>, fm: &PhotoSorterFrontMatter) -> String {
    let mut root: Hash;
    if let Some(s) = s {
        let yaml_docs_r = YamlLoader::load_from_str(s);
        let Ok(yaml_docs) = yaml_docs_r else {
            warn!("Could not parse YAML: {s}");
            return s.to_string();
        };
        let yaml_doc_o = yaml_docs.first();
        let Some(yaml_doc) = yaml_doc_o else {
            warn!("No YAML document found in: {s}");
            return s.to_string();
        };
        let Yaml::Hash(hash) = &yaml_doc else {
            warn!("Root YAML is not a hash {yaml_doc:?}");
            return s.to_string();
        };
        root = hash.clone();
    } else {
        root = Hash::default();
    }
    yaml_array_merge(&mut root, &"original-paths".to_string(), &fm.path_original);

    // todo: add longitude, latitude and people
    // todo: add exif datetime, gps date, unique id

    let mut out_str = String::new();
    {
        let mut emitter = YamlEmitter::new(&mut out_str);
        let yaml_hash = Yaml::Hash(root);
        emitter.dump(&yaml_hash).unwrap();
    }
    out_str = out_str.trim_start_matches("---").to_string();
    out_str = out_str.trim_start_matches("\n").to_string();
    out_str = out_str.trim_end_matches("\n").to_string();
    out_str += "\n";
    out_str
}

fn yaml_array_merge(root: &mut Hash, key: &String, arr: &Vec<String>) {
    if let Some(value_o) = root.get(&Yaml::String(key.clone())) {
        match value_o.clone() {
            Yaml::Array(po) => {
                let mut new_po = po.clone();
                for v in arr {
                    if po.contains(&Yaml::String(v.clone())) {
                        debug!("Path original {v} already exists in {key}");
                    } else {
                        debug!("Adding {v} to {key}");
                        new_po.push(Yaml::String(v.to_string()));

                    }
                }
                root[&Yaml::String(key.to_string())] = Yaml::Array(new_po);
                return;
            }
            Yaml::BadValue => {
                // fall through as current value is empty/unknown
            }
            _ => {
                warn!("Expected {key} to be an array, found: {value_o:?}");
                return;
            }
        }
    }
    debug!("Adding {key} to YAML");
    let arr_y = arr.clone()
        .iter().map(|x| Yaml::String(x.to_string())).collect::<Vec<Yaml>>();
    root.insert(Yaml::String(key.to_string()), Yaml::Array(arr_y));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_mfi() -> PhotoSorterFrontMatter {
        PhotoSorterFrontMatter {
            path_original: vec!["p1".to_string(), "p2".to_string()],
            datetime_original: None,
            datetime: None,
            gps_date: None,
            unique_id: None,
        }
    }


    #[test]
    fn test_yaml_output() {
        crate::test_util::setup_log();
        let s = "foo:
  - list1
".to_string();
        let yaml = merge_yaml(&Some(s), &get_mfi());
        assert_eq!(yaml, "foo:
  - list1
original-paths:
  - p1
  - p2
");
    }

    #[test]
    fn test_yaml_output_existing() {
        crate::test_util::setup_log();
        let s = "foo:
  - list1
original-paths:
  - p0
".to_string();
        let yaml = merge_yaml(&Some(s), &get_mfi());
        assert_eq!(yaml, "foo:
  - list1
original-paths:
  - p0
  - p1
  - p2
");
    }

//     #[test]
//     fn test_parse_frontmatter() {
//         crate::test_util::setup_log();
//         let (fm_o, md) = parse_frontmatter("---
//   photo-sorter:
//     path: 2025/02/09/1123-23-abcdefg.jpg
//     path-original:
//       - Google Photos/Photos from 2025/IMG_5071.HEIC
//     datetime: 2025-02-09T18:17:01Z
//     gps-date: 2025-02-09
// ---
// x
// last line", "test.md");
//         assert_eq!(fm_o.unwrap(), PhotoSorterFrontMatter {
//             path: Some("2025/02/09/1123-23-abcdefg.jpg".to_string()),
//             path_original: vec!["Google Photos/Photos from 2025/IMG_5071.HEIC".to_string()],
//             datetime_original: None,
//             datetime: Some("2025-02-09T18:17:01Z".to_string()),
//             gps_date: Some("2025-02-09".to_string()),
//             unique_id: None,
//         });
//         assert_eq!(md, "x\nlast line".to_string());
//     }

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
}

