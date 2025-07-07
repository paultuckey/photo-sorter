# photo-sorter-cli

Problem: There is no shared format for archiving photos and videos, this tool aims to provide a simple solution that 
works with both Google Takeout and iCloud archives, while being robust, portable and future-proof.

This is a CLI tool to organise photos, videos and albums from Google Takeout and iCloud zip files (or directories) and 
sort them into directories based on their EXIF metadata and any supplemental info.

In detail:
- Files are put into directories with the following format: `yyyy/mm/dd/hhmm-ss-{short checksum}.ext`
- For each photo or video file:
  - A matching Markdown file is written at the same path with the extension `md`
  - This contains [YAML](https://en.wikipedia.org/wiki/YAML) frontmatter (the part between `---`'s) with metadata (based on EXIF tags)
  - The Markdown part of this file can be edited with notes, and it will not be clobbered on later runs
  - Determine date based on EXIF tags or file modification time
- Rename files with the wrong extension based on a inspecting bytes of the file
- For each Album (Google uses JSON format, iCloud CSV) a Markdown file will be produced
- Input can be Google Takeout zip/directory or iCloud archive zip or directory
- Sync photos/videos into existing directories without clobbering if the same file exists already
  - Additive only nothing will be deleted or overwritten

## FAQ

> Why use date based file and directory names? Why include the checksum in the file name?

Time is the most important factor in archiving, it enables you to take different actions with different year 
directories. 

A robust failsafe solution for file naming is needed that will be durable _very_ long term. Multiple photos can be 
taken during the same second, the checksum is used to differentiate them (date-based EXIF tags do not provide sub-second accuracy).

> Why use markdown files?

Markdown is widely supported and human readable without any special software.

## Usage

```shell
cargo run -- --help
```

```shell
cargo run -- markdown --debug --input "test/Canon_40D.jpg"
```

```shell
cargo run -- \
  sync --debug --dry-run \
    --input "input/takeout-20250614T030613Z-1-001.zip" \
    --output "output/archive"
```

```shell
cargo run -- sync --debug --input "input/Takeout-small" --output "output/archive-small"
```


---

Google is a trademark of Google LLC. iCloud is a trademark of Apple Inc.