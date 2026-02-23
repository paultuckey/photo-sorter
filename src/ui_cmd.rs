use crate::fs::{FileSystem, OsFileSystem, ZipFileSystem};
use anyhow::anyhow;
use eframe::egui;
use rusqlite::Connection;
use std::path::Path;

struct UiApp {
    image_bytes: egui::load::Bytes,
    image_path: String,
}

impl UiApp {
    fn new(image_path: String, image_bytes: Vec<u8>) -> Self {
        Self {
            image_bytes: image_bytes.into(),
            image_path,
        }
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::from_bytes(
                    format!("bytes://{}", self.image_path),
                    self.image_bytes.clone(),
                ));
            });
        });
    }
}

fn get_first_jpeg(conn: &Connection) -> anyhow::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT media_path FROM media_item WHERE accurate_file_type = 'Jpg' LIMIT 1")?;
    let mut rows = stmt.query([])?;

    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

fn load_image_bytes(input: &str, media_path: &str) -> anyhow::Result<Vec<u8>> {
    let path = Path::new(input);
    let container: Box<dyn FileSystem> = if path.is_dir() {
        Box::new(OsFileSystem::new(input))
    } else {
        let tz = chrono::Local::now().offset().to_owned();
        Box::new(ZipFileSystem::new(input, tz)?)
    };

    let mut reader = container.open(media_path)?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn main(input: &str) -> anyhow::Result<()> {
    let conn = Connection::open("db.sqlite")?;
    let media_path_opt = get_first_jpeg(&conn)?;

    let media_path = if let Some(p) = media_path_opt {
        p
    } else {
        println!("No JPEGs found in database.");
        return Ok(());
    };

    let bytes = load_image_bytes(input, &media_path)?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true),
        ..Default::default()
    };

    eframe::run_native(
        "Photo Sorter",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(UiApp::new(media_path, bytes)))
        }),
    ).map_err(|e| anyhow!("Eframe error: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_get_first_jpeg_found() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE media_item (media_path TEXT, accurate_file_type TEXT)",
            (),
        )?;
        conn.execute(
            "INSERT INTO media_item (media_path, accurate_file_type) VALUES ('photo1.jpg', 'Jpg')",
            (),
        )?;
        conn.execute(
            "INSERT INTO media_item (media_path, accurate_file_type) VALUES ('video1.mp4', 'Mp4')",
            (),
        )?;

        let result = get_first_jpeg(&conn)?;
        assert_eq!(result, Some("photo1.jpg".to_string()));
        Ok(())
    }

    #[test]
    fn test_get_first_jpeg_not_found() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE media_item (media_path TEXT, accurate_file_type TEXT)",
            (),
        )?;
        conn.execute(
            "INSERT INTO media_item (media_path, accurate_file_type) VALUES ('video1.mp4', 'Mp4')",
            (),
        )?;

        let result = get_first_jpeg(&conn)?;
        assert_eq!(result, None);
        Ok(())
    }
}
