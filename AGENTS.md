# AGENTS.md

This file provides context and guidelines for AI agents and developers working on the `photo-sorter` codebase.

## Project Overview

`photo-sorter` is a CLI tool designed to organize photos and videos from Google Takeout and iCloud archives. It standardizes directory structures (year/month/day), handles duplicates via checksums, and generates Markdown files with metadata for each media item and album.

## Tech Stack

- **Language**: Rust
- **CLI Framework**: `clap`
- **Logging**: `tracing`
- **Database**: `rusqlite` (SQLite)
- **Error Handling**: `anyhow`
- **Serialization**: `serde`, `serde_json`, `yaml-rust2`
- **Regex**: `regex` (for file pattern matching)

## Development Guidelines

Follow these principles to keep the codebase approachable and maintainable:

1.  **Simplicity First**: Avoid complex Rust features unless absolutely necessary.
    -   Do **not** use lifetimes explicitly if possible.
    -   Do **not** use `unsafe` code.
    -   Do **not** use `async`/`await`.
    -   Do **not** use `expect()` or `unwrap()` in production code; use proper error handling.
2.  **Error Handling**:
    -   Use `anyhow::Result` for functions that can fail.
    -   Propagate errors with `?`.
3.  **Memory Management**:
    -   Use `.clone()` liberally to simplify ownership and borrowing issues. Performance is secondary to readability and simplicity for this tool.
4.  **Testing**:
    -   Write unit tests within the module (typically at the bottom of the file in a `tests` module).
    -   Use `crate::test_util::setup_log()` at the beginning of tests to enable logging output.

## Project Structure

The source code is located in `src/`:

-   **`main.rs`**: Entry point. Defines the CLI structure using `clap` and orchestrates subcommands.
-   **`*_cmd.rs`**: Implementations for specific CLI commands:
    -   `sync_cmd.rs`: Core logic for syncing files, deduplication, and writing to the output directory.
    -   `db_cmd.rs`: Logic for scanning files and populating a SQLite database with metadata.
    -   `index_cmd.rs`: Analyzes input directories/zips to identify known file and directory patterns using regex.
    -   `info_cmd.rs`: Inspects and displays details for a single file.
-   **`media.rs`**: Core data structures (`MediaFileInfo`, `MediaFileDerivedInfo`) and logic for extracting metadata, calculating checksums, and deriving target paths/dates.
-   **`album.rs`**: Logic for parsing album metadata (from CSV or JSON) and generating album Markdown files.
-   **`markdown.rs`**: Utilities for reading/writing Markdown files and managing YAML frontmatter.
-   **`util.rs`**: General utilities, including the `PsContainer` trait which abstracts file system access (supporting both directories and zip files).
-   **`test_util.rs`**: Helper functions for testing, primarily logging setup.

## Common Commands

Refer to `Development.md` for a comprehensive list of commands. Key commands include:

-   **Run**: `cargo run -- <command> <args>`
-   **Test**: `cargo test`
-   **Format**: `cargo fmt`
-   **Lint**: `cargo clippy`

## Key Concepts

-   **PsContainer**: An abstraction to treat directories and zip files uniformly.
-   **ScanInfo**: Basic information about a file found during a scan.
-   **MediaFileInfo**: detailed metadata about a media file (EXIF, checksum, etc.).
-   **Supplemental Info**: JSON sidecar files (often from Google Takeout) containing metadata like creation time and GPS coordinates.
