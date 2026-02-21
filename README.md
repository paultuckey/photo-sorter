# Photo Sorter CLI

> [!WARNING]
> This tool is in active development, use at your own risk.

Problem: Google Takeout and iCloud archives of photos and videos:
- do _not_ share a common standard for directory and photo naming
- do _not_ represent albums in a standardized way
- do _not_ allow for duplicates to be merged

Solution: A CLI tool that syncs photos, videos and albums from Google Takeout and iCloud archives
into a standard directory structure that:
- Separates photos by year to makes long-term archiving by year possible
- checksums files to avoid storing duplicates
- Standardizes file names based on EXIF tags
- Standardizes albums as Markdown files

In detail:
- EXIF metadata and supplemental is extracted from photos and videos and used to determine the date and time of the file
- Files are put into directories with the following format: `yyyy/mm/dd/hhmmss-ms{-duplicate}.ext`
- For each photo or video file:
  - A matching Markdown file is written at the same path with the extension `md`
  - This contains [YAML](https://en.wikipedia.org/wiki/YAML) frontmatter (the part between `---`'s) where basic metadata is written
  - The Markdown part of this file can be edited with notes, and it will not be clobbered on later runs
  - Determine date and time taken based on EXIF tags or file modification time
- Rename files with the wrong extension based on a inspecting bytes of the file
- For each Album (Google uses JSON format, iCloud CSV) a Markdown file will be produced
- Input can be Google Takeout zip/directory or iCloud archive zip or directory
- Sync photos/videos into existing directories without clobbering if the same file exists already
  - Additive only nothing will be deleted or overwritten


## Installation

You will need to install Rust and Cargo, follow the instructions on the [Rust installation page](https://www.rust-lang.org/tools/install).

Then build the project from source.

```shell
cargo install --git https://github.com/paultuckey/photo-sorter.git photo-sorter
```

## Usage

```shell
photo-sorter --help
```

```shell
photo-sorter info --debug --root "test" --input "Canon_40D.jpg"
```

```shell
photo-sorter \
  sync --debug --dry-run \
    --input "input/takeout-20250614T030613Z-1-001.zip" \
    --output "output/archive"
```

```shell
photo-sorter sync --debug --input "input/Takeout-small" --output "output/archive-small"
```

## FAQ

> Why use date based file and directory names? Why include the checksum in the file name?

Time is the most important factor in archiving, it enables you to take different actions with different year
directories.

A robust failsafe solution for file naming is needed that will be durable _very_ long term. Multiple photos can be
taken during the same second, the checksum is used to differentiate them (date-based EXIF tags do not provide sub-second accuracy).

> Why use markdown files?

Markdown is widely supported and human readable without any special software. Just as with
[Obsidian](https://obsidian.md/), you can edit the Markdown files with any text editor, or backup the directoryies to
any storage solution.

> What format is the short checksum?

It's the first 7 characters of a SHA256 hash over the bytes of the file. As with a git short hash it's a good trade-off
between uniqueness and length.

> What is the YAML in the Markdown files for?

Two reasons:
- It allows for notes to be made on each photo or album that will not be clobbered on later runs
- It allows for metadata to be stored in a structured way that can be easily parsed by software (in frontmatter)

---

Google is a trademark of Google LLC. iCloud is a trademark of Apple Inc.