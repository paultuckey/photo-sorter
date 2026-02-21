use crate::file_type::QuickFileType;
use crate::media::{MediaFileInfo, best_guess_taken_dt, media_file_info_from_readable};
use crate::progress::Progress;
use crate::supplemental_info::{detect_supplemental_info, load_supplemental_info};
use crate::util::{PsContainer, PsDirectoryContainer, PsZipContainer, ScanInfo, checksum_bytes};
use anyhow::anyhow;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

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
    run_db_scan(&mut container, &conn)?;
    conn.close().unwrap_or(());
    Ok(())
}

fn run_db_scan(
    container: &mut Box<dyn PsContainer>,
    conn: &Connection,
) -> anyhow::Result<()> {
    db_prepare(conn)?;

    let files = container.scan();
    info!("Found {} files in input", files.len());

    let media_si_files = files
        .iter()
        .filter(|m| m.quick_file_type == QuickFileType::Media)
        .collect::<Vec<&ScanInfo>>();
    info!("Inspecting {} photo and video files", media_si_files.len());
    let prog = Progress::new(media_si_files.len() as u64);
    for media_si in media_si_files {
        prog.inc();
        process_file(container, conn, &media_si)?;
    }
    drop(prog);

    // todo: support albums

    info!("Done {} files", files.len());
    Ok(())
}

fn process_file(
    root: &mut Box<dyn PsContainer>,
    conn: &Connection,
    media_si: &&ScanInfo,
) -> anyhow::Result<()> {
    let mut supp_info_o = None;
    let supp_info_path_o = detect_supplemental_info(&media_si.file_path.clone(), root);
    if let Some(supp_info_path) = supp_info_path_o {
        supp_info_o = load_supplemental_info(&supp_info_path, root);
    }

    let reader = root.file_reader(&media_si.file_path.clone())?;
    let hash_info_o = checksum_bytes(reader).ok();
    let Some(hash_info) = hash_info_o else {
        debug!(
            "Could not calculate checksum for file: {:?}",
            media_si.file_path
        );
        return Err(anyhow!(
            "Could not calculate checksum for file: {:?}",
            media_si.file_path
        ));
    };

    let media_info_r = media_file_info_from_readable(media_si, root, &supp_info_o, &hash_info);
    if let Ok(media_info) = media_info_r {
        db_record(conn, &media_info)?;
    }
    Ok(())
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase", serialize = "camelCase"))]
pub(crate) struct HashInfo {
    pub(crate) short_checksum: String,
    pub(crate) long_checksum: String,
}

fn db_record(conn: &Connection, info: &MediaFileInfo) -> anyhow::Result<()> {
    let media_info_json = serde_json::to_string(&info)?;
    let guessed_datetime = best_guess_taken_dt(info);
    let long_hash = &info.hash_info.long_checksum;
    let short_hash = &info.hash_info.short_checksum;
    let item = DbMediaItem {
        media_path: info.original_file_this_run.clone(),
        long_hash: long_hash.to_string(),
        short_hash: short_hash.to_string(),
        media_info: Some(media_info_json),
        modified_at: info.modified.unwrap_or(0),
        created_at: info.created.unwrap_or(0),
        quick_file_type: info.quick_file_type.clone().to_string(),
        accurate_file_type: info.accurate_file_type.clone().to_string(),
        guessed_datetime,
    };
    conn.execute(
        DB_MEDIA_ITEM_INSERT,
        (
            &item.media_path,
            &item.long_hash,
            &item.short_hash,
            &item.quick_file_type,
            &item.accurate_file_type,
            &item.media_info,
            &item.guessed_datetime,
            &item.modified_at,
            &item.created_at,
        ),
    )?;

    Ok(())
}

#[derive(Debug)]
struct DbMediaItem {
    media_path: String,
    long_hash: String,
    short_hash: String,
    media_info: Option<String>,
    quick_file_type: String,
    accurate_file_type: String,
    // formatted as ISO 8601
    guessed_datetime: Option<String>,
    modified_at: i64,
    created_at: i64,
}
const DB_MEDIA_ITEM_CREATE: &str = "
    CREATE TABLE IF NOT EXISTS media_item  (
        media_item_id INTEGER PRIMARY KEY AUTOINCREMENT,
        media_path TEXT NOT NULL,
        long_hash TEXT,
        short_hash TEXT,
        quick_file_type TEXT,
        accurate_file_type TEXT,
        media_info TEXT,
        guessed_datetime DATETIME,
        modified_at DATETIME DEFAULT CURRENT_TIMESTAMP, -- file last modified
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP -- file created
    )
";
const DB_MEDIA_ITEM_INSERT: &str = "
    INSERT INTO media_item (media_path, long_hash, short_hash, quick_file_type,
        accurate_file_type, media_info, guessed_datetime, modified_at, created_at)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
";
#[allow(dead_code)]
const DB_MEDIA_ITEM_SELECT_ALL: &str = "
    SELECT media_path, long_hash, short_hash, quick_file_type,
        accurate_file_type, media_info, guessed_datetime, modified_at, created_at
    FROM media_item
";
const DB_MEDIA_ITEM_DELETE_ALL: &str = "
    DELETE FROM media_item
";

fn db_conn() -> anyhow::Result<Connection> {
    Ok(Connection::open("db.sqlite")?)
}

fn db_prepare(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(DB_MEDIA_ITEM_CREATE, ())?;
    conn.execute(DB_MEDIA_ITEM_DELETE_ALL, ())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_select_all() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let conn = db_conn()?;
        let mut res = conn.prepare(DB_MEDIA_ITEM_SELECT_ALL)?;
        let mut rows = res.query(())?;
        while let Some(row) = rows.next()? {
            let media_path: String = row.get(0)?;
            println!("media_path: {}", media_path);
        }
        Ok(())
    }

    #[test]
    fn test_db_scan() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let conn = Connection::open_in_memory()?;
        let mut container: Box<dyn PsContainer> = Box::new(PsDirectoryContainer::new("test"));
        run_db_scan(&mut container, &conn)?;

        let mut stmt = conn
            .prepare("SELECT media_path, quick_file_type FROM media_item ORDER BY media_path")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        assert!(results
            .iter()
            .any(|(path, ftype)| path == "Canon_40D.jpg" && ftype == "Media"));
        assert!(results
            .iter()
            .any(|(path, ftype)| path == "Hello.mp4" && ftype == "Media"));

        Ok(())
    }
}
