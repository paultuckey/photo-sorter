use crate::exif::{ExifTag, all_tags};
use crate::file_type::QuickFileType;
use crate::media::MediaFileInfo;
use crate::supplemental_info::{detect_supplemental_info, load_supplemental_info};
use crate::sync_cmd::inspect_media;
use crate::util::{Progress, PsContainer, PsDirectoryContainer, PsZipContainer, ScanInfo};
use anyhow::anyhow;
use log::{debug, info, warn};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn main(input: &String) -> anyhow::Result<()> {
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

    let conn = db_conn()?;
    db_prepare(&conn)?;

    let files = container.scan();
    info!("Found {} files in input", files.len());

    let mut all_media = HashMap::<String, MediaFileInfo>::new();
    let media_si_files = files
        .iter()
        .filter(|m| m.quick_file_type == QuickFileType::Media)
        .collect::<Vec<&ScanInfo>>();
    info!("Inspecting {} photo and video files", media_si_files.len());
    let prog = Progress::new(media_si_files.len() as u64);
    for media_si in media_si_files {
        prog.inc();

        let mut supp_info_o = None;
        let supp_info_path_o =
            detect_supplemental_info(&media_si.file_path.clone(), container.as_ref());
        if let Some(supp_info_path) = supp_info_path_o {
            supp_info_o = load_supplemental_info(&supp_info_path, &mut container);
        }
        let bytes = container.file_bytes(&media_si.file_path.clone());
        let Ok(bytes) = bytes else {
            warn!("Could not read file: {}", media_si.file_path);
            return Err(anyhow!("Could not read file: {}", media_si.file_path));
        };
        let media_info_r = inspect_media(&bytes, media_si, &mut all_media, &supp_info_o);
        if let Ok(media_info) = media_info_r {
            let tags = all_tags(&bytes);
            db_record(&conn, &media_info, &tags)?;
        }
    }
    drop(prog);

    // todo: support albums

    info!("Done {} files", files.len());
    conn.close().unwrap_or(());
    Ok(())
}

fn db_record(conn: &Connection, info: &MediaFileInfo, tags: &Vec<ExifTag>) -> anyhow::Result<()> {
    let item = DbMediaItem {
        desired_path: info.desired_media_path.clone().unwrap_or("".to_string()),
        long_hash: info.long_checksum.clone(),
        short_hash: info.short_checksum.clone().to_string(),
        modified_at: info.modified.unwrap_or(0),
        quick_file_type: info.quick_file_type.clone().to_string(),
        accurate_file_type: info.accurate_file_type.clone().to_string(),
        guessed_datetime: info.guessed_datetime.unwrap_or(0),
    };
    let mut stmt = conn.prepare("SELECT media_item_id FROM media_item WHERE long_hash = ?1")?;

    let existing_r = stmt.query_one([item.long_hash.clone()], |row| row.get(0));
    let media_item_id;
    if let Ok(existing) = existing_r {
        media_item_id = existing;
    } else {
        conn.execute(
            DB_MEDIA_ITEM_INSERT,
            (
                &item.desired_path,
                &item.long_hash,
                &item.short_hash,
                &item.quick_file_type,
                &item.accurate_file_type,
                &item.guessed_datetime,
                &item.modified_at,
            ),
        )?;
        // get id for inserted item
        media_item_id = conn.last_insert_rowid() as i32;
    }

    for op in &info.original_path {
        let db_ai = DbArchiveItem {
            media_item_id,
            path: op.to_string(),
        };
        conn.execute(DB_ARCHIVE_ITEM_INSERT, (&db_ai.media_item_id, &db_ai.path))?;
    }

    for t in tags {
        let db_t = DbExifTag {
            media_item_id,
            name: t.tag_code.clone(),
            value: t.tag_value.clone().unwrap_or("".to_string()),
            tag_type: t.tag_type.clone().unwrap_or("".to_string()),
        };
        conn.execute(
            DB_EXIF_TAG_INSERT,
            (&db_t.media_item_id, &db_t.name, &db_t.value, &db_t.tag_type),
        )?;
    }
    Ok(())
}

#[derive(Debug)]
struct DbMediaItem {
    desired_path: String,
    long_hash: String,
    short_hash: String,
    quick_file_type: String,
    accurate_file_type: String,
    guessed_datetime: i64,
    modified_at: i64,
}
// todo: make desired, hashes unique
const DB_MEDIA_ITEM_CREATE: &str = "
    CREATE TABLE IF NOT EXISTS media_item  (
        media_item_id INTEGER PRIMARY KEY AUTOINCREMENT,
        desired_path TEXT NOT NULL,
        long_hash TEXT,
        short_hash TEXT,
        quick_file_type TEXT,
        accurate_file_type TEXT,
        guessed_datetime DATETIME,
        modified_at DATETIME DEFAULT CURRENT_TIMESTAMP
    )
";
const DB_MEDIA_ITEM_INSERT: &str = "
    INSERT INTO media_item (desired_path, long_hash, short_hash, quick_file_type, accurate_file_type, guessed_datetime, modified_at)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
";
const DB_MEDIA_ITEM_DELETE_ALL: &str = "
    DELETE FROM media_item
";

#[derive(Debug)]
struct DbArchiveItem {
    media_item_id: i32,
    path: String,
}
// todo: make desired, hashes unique
const DB_ARCHIVE_ITEM_CREATE: &str = "
    CREATE TABLE IF NOT EXISTS archive_item (
        archive_item_id INTEGER PRIMARY KEY AUTOINCREMENT,
        media_item_id INTEGER,
        path TEXT NOT NULL
    )
";
const DB_ARCHIVE_ITEM_INSERT: &str = "
    INSERT INTO archive_item (media_item_id, path)
    VALUES (?1, ?2)
";
const DB_ARCHIVE_ITEM_DELETE_ALL: &str = "
    DELETE FROM archive_item
";

#[derive(Debug)]
struct DbExifTag {
    media_item_id: i32,
    name: String,
    value: String,
    tag_type: String,
}
const DB_EXIF_TAG_CREATE: &str = "
    CREATE TABLE IF NOT EXISTS exif_tag (
        exif_tag_id INTEGER PRIMARY KEY AUTOINCREMENT,
        media_item_id INTEGER NOT NULL,
        tag_name TEXT NOT NULL,
        tag_value TEXT,
        tag_type TEXT, -- 'string', 'integer', 'rational', 'datetime', etc.
        ifd_name TEXT, -- IFD0, Exif, GPS, etc.
        FOREIGN KEY (media_item_id) REFERENCES media_item(media_item_id) ON DELETE CASCADE,
        UNIQUE(media_item_id, tag_name, ifd_name)
    )
";
const DB_EXIF_TAG_INSERT: &str = "
    INSERT INTO exif_tag (media_item_id, tag_name, tag_value, tag_type)
    VALUES (?1, ?2, ?3, ?4)
";
const DB_EXIF_TAG_DELETE_ALL: &str = "
    DELETE FROM exif_tag
";

fn db_conn() -> anyhow::Result<Connection> {
    Ok(Connection::open("db.sqlite")?)
}

fn db_prepare(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(DB_MEDIA_ITEM_CREATE, ())?;
    conn.execute(DB_ARCHIVE_ITEM_CREATE, ())?;
    conn.execute(DB_EXIF_TAG_CREATE, ())?;

    conn.execute(DB_MEDIA_ITEM_DELETE_ALL, ())?;
    conn.execute(DB_ARCHIVE_ITEM_DELETE_ALL, ())?;
    conn.execute(DB_EXIF_TAG_DELETE_ALL, ())?;
    Ok(())
}
