use crate::file_type::QuickFileType;
use crate::fs::{FileSystem, OsFileSystem, ZipFileSystem};
use crate::media::{MediaFileInfo, best_guess_taken_dt, media_file_info_from_readable};
use crate::progress::Progress;
use crate::supplemental_info::{detect_supplemental_info, load_supplemental_info};
use crate::util::{ScanInfo, checksum_bytes, scan_fs};
use anyhow::anyhow;
use rayon::prelude::*;
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
    let mut container: Box<dyn FileSystem>;
    if path.is_dir() {
        info!("Input directory: {input}");
        container = Box::new(OsFileSystem::new(input));
    } else {
        info!("Input zip: {input}");
        let tz = chrono::Local::now().offset().to_owned();
        container = Box::new(ZipFileSystem::new(input, tz)?);
    }

    let conn = db_conn()?;
    run_db_scan(&mut container, &conn)?;
    conn.close().unwrap_or(());
    Ok(())
}

fn run_db_scan(container: &mut Box<dyn FileSystem>, conn: &Connection) -> anyhow::Result<()> {
    db_prepare(conn)?;

    let files = scan_fs(container.as_ref());
    info!("Found {} files in input", files.len());

    let media_si_files = files
        .iter()
        .filter(|m| m.quick_file_type == QuickFileType::Media)
        .collect::<Vec<&ScanInfo>>();
    info!("Inspecting {} photo and video files", media_si_files.len());
    let prog = Progress::new(media_si_files.len() as u64);

    std::thread::scope(|s| {
        let (tx, rx) = std::sync::mpsc::channel();
        let container_ref = container.as_ref();
        let prog_ref = &prog;

        s.spawn(move || {
            media_si_files.par_iter().for_each(|media_si| {
                if let Ok(Some(info)) = analyze_file(container_ref, media_si) {
                    let _ = tx.send(info);
                }
                prog_ref.inc();
            });
        });

        for info in rx {
            db_record(conn, &info)?;
        }
        Ok::<(), anyhow::Error>(())
    })?;

    drop(prog);

    // todo: support albums

    info!("Done {} files", files.len());
    Ok(())
}

fn analyze_file(
    root: &dyn FileSystem,
    media_si: &ScanInfo,
) -> anyhow::Result<Option<MediaFileInfo>> {
    let mut supp_info_o = None;
    let supp_info_path_o = detect_supplemental_info(&media_si.file_path.clone(), root);
    if let Some(supp_info_path) = supp_info_path_o {
        supp_info_o = load_supplemental_info(&supp_info_path, root);
    }

    let reader = root.open(&media_si.file_path.clone())?;
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
    match media_info_r {
        Ok(media_info) => Ok(Some(media_info)),
        Err(_) => Ok(None),
    }
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

    const DB_MEDIA_ITEM_SELECT_ALL: &str = "
        SELECT media_path, long_hash, short_hash, quick_file_type,
            accurate_file_type, media_info, guessed_datetime, modified_at, created_at
        FROM media_item
    ";

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
        let mut container: Box<dyn FileSystem> = Box::new(OsFileSystem::new("test"));
        run_db_scan(&mut container, &conn)?;

        let mut stmt =
            conn.prepare("SELECT media_path, quick_file_type FROM media_item ORDER BY media_path")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        assert!(
            results
                .iter()
                .any(|(path, ftype)| path == "Canon_40D.jpg" && ftype == "Media")
        );
        assert!(
            results
                .iter()
                .any(|(path, ftype)| path == "Hello.mp4" && ftype == "Media")
        );

        Ok(())
    }

    use std::fs;
    use zip::ZipWriter;
    use zip::write::FileOptions;

    fn create_zip_of_test_dir(output_path: &Path) -> anyhow::Result<()> {
        let file = fs::File::create(output_path)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default();

        let root = Path::new("test");
        let walker = fs::read_dir(root)?;
        for entry in walker {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap().to_str().unwrap();
                zip.start_file(name, options)?;
                let mut f = fs::File::open(&path)?;
                std::io::copy(&mut f, &mut zip)?;
            }
        }
        zip.finish()?;
        Ok(())
    }

    #[test]
    fn test_db_scan_zip() -> anyhow::Result<()> {
        crate::test_util::setup_log();
        let zip_path = Path::new("target/test_output.zip");
        create_zip_of_test_dir(zip_path)?;

        let conn = Connection::open_in_memory()?;
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let mut container: Box<dyn FileSystem> = Box::new(ZipFileSystem::new(
            &zip_path.to_string_lossy().to_string(),
            tz,
        )?);

        run_db_scan(&mut container, &conn)?;

        let mut stmt =
            conn.prepare("SELECT media_path, quick_file_type FROM media_item ORDER BY media_path")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        assert!(
            results
                .iter()
                .any(|(path, ftype)| path == "Canon_40D.jpg" && ftype == "Media")
        );
        assert!(
            results
                .iter()
                .any(|(path, ftype)| path == "Hello.mp4" && ftype == "Media")
        );

        // Cleanup
        let _ = fs::remove_file(zip_path);
        Ok(())
    }
}
