# photo-sorter-cli

A CLI tool to take photos from Google Takeout and iCloud zips (or directory) and sort them into directories 
based on their EXIF metadata and any supplemental info.

- Input
  - Google Takeout zip or a directory
  - iCloud archive zip or a directory
- Output
  - Markdown file with frontmatter per photo/video and album
  - Each Album as a markdown file
  - Sync photos/videos from input into existing directories 
    - without clobbering if same checksum ignore
    - only updating frontmatter in existing markdown files
    - additive only nothing will be deleted or overwritten

yyyy/mm/dd/hhmm-ss[-i].ext
yyyy/mm/dd/hhmm-ss[-i].md

## FAQ

> Why date based directories?

Time is the most important factor in archiving.

> Why markdown?

Widely supported and human readable witout any special software.

Markdown files contain date information from exif used to determine file location.

You can safely edit the markdown files to change the information. 
When running the tool again, this will not be clobbered.


## Markdown

Example:

```markdown
---
photo-lister:
    checksum: xxx
    date: 2025-02-28
    origin: xyz.takeout.zip
---

Regular _Markdown_ follows...
```

## Usage

```shell
cargo run -- --help
```

```shell
cargo run -- \
  markdown --debug \
    --input "test/Canon_40D.jpg"
```


```shell
cargo run -- \
  sync --debug --dry-run \
    --input "/Users/paul/Downloads/takeout-20250614T030613Z-1-001.zip" \
    --output "out/archive"
```

## Development

- Don't use lifetimes
- Don't use `unsafe`, `expect()` or `unwrap()`
- Use `.clone()` to avoid hard things in Rust.

```shell
cargo clippy
```

```shell
cargo run
```

```shell
cargo test
```
