use crate::exif::{ExifInfo, best_guess_taken_dt, exif_info};
use crate::file_type::QuickFileType;
use crate::media::{MediaFileInfo, media_file_info_from_readable};
use crate::supplemental_info::{detect_supplemental_info, load_supplemental_info};
use crate::util::{
    Progress, PsContainer, PsDirectoryContainer, PsZipContainer, ScanInfo, checksum_bytes,
};
use anyhow::anyhow;
use log::{debug, info, warn};
use rusqlite::Connection;
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
        let checksum_o = checksum_bytes(&bytes).ok();
        let Some((short_checksum, long_checksum)) = checksum_o else {
            debug!(
                "Could not calculate checksum for file: {:?}",
                media_si.file_path
            );
            return Err(anyhow!(
                "Could not calculate checksum for file: {:?}",
                media_si.file_path
            ));
        };
        let media_info_r = media_file_info_from_readable(
            media_si,
            &bytes,
            &supp_info_o,
            &short_checksum,
            &long_checksum,
        );
        if let Ok(media_info) = media_info_r {
            let exif_info_s = exif_info(&bytes);
            db_record(&conn, &media_info, &exif_info_s)?;
        }
    }
    drop(prog);

    // todo: support albums

    info!("Done {} files", files.len());
    conn.close().unwrap_or(());
    Ok(())
}

fn db_record(conn: &Connection, info: &MediaFileInfo, exif_info: &ExifInfo) -> anyhow::Result<()> {
    let supp_info_json = info
        .supp_info
        .clone()
        .map(|supp_info| serde_json::to_string(&supp_info).unwrap_or("".to_string()));
    let exif_json = serde_json::to_string(&exif_info).unwrap_or("".to_string());
    let guessed_datetime = best_guess_taken_dt(
        &info.parsed_exif,
        &info.supp_info,
        info.modified,
        info.created,
    );
    let item = DbMediaItem {
        media_item_id: 0,
        media_path: info.original_file_this_run.clone(),
        long_hash: info.long_checksum.clone(),
        short_hash: info.short_checksum.clone().to_string(),
        exif_json,
        supp_info_json: supp_info_json.clone(),
        modified_at: info.modified.unwrap_or(0),
        created_at: info.created.unwrap_or(0),
        quick_file_type: info.quick_file_type.clone().to_string(),
        accurate_file_type: info.accurate_file_type.clone().to_string(),
        guessed_datetime: guessed_datetime.unwrap_or(0),
    };
    conn.execute(
        DB_MEDIA_ITEM_INSERT,
        (
            &item.media_path,
            &item.long_hash,
            &item.short_hash,
            &item.quick_file_type,
            &item.accurate_file_type,
            &item.exif_json,
            &item.supp_info_json,
            &item.guessed_datetime,
            &item.modified_at,
            &item.created_at,
        ),
    )?;

    Ok(())
}

#[derive(Debug)]
struct DbMediaItem {
    media_item_id: i64,
    media_path: String,
    long_hash: String,
    short_hash: String,
    exif_json: String,
    supp_info_json: Option<String>,
    quick_file_type: String,
    accurate_file_type: String,
    guessed_datetime: i64,
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
        exif_json TEXT,
        supp_info_json TEXT,
        guessed_datetime DATETIME,
        modified_at DATETIME DEFAULT CURRENT_TIMESTAMP, -- file last modified
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP -- file created
    )
";
const DB_MEDIA_ITEM_INSERT: &str = "
    INSERT INTO media_item (media_path, long_hash, short_hash, quick_file_type,
        accurate_file_type, exif_json, supp_info_json, guessed_datetime, modified_at, created_at)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
";
const DB_MEDIA_ITEM_SELECT_ALL: &str = "
    SELECT media_path, long_hash, short_hash, quick_file_type,
        accurate_file_type, exif_json, supp_info_json, guessed_datetime, modified_at, created_at
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
}
