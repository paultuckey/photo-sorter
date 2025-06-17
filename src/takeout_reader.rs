use std::{fs, io};
use tracing::info;

fn main(s: &str, dry_run: &bool) {
    let fname = std::path::Path::new(s);
    let file = fs::File::open(fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let out_path = match file.enclosed_name() {
            Some(path) => path,
            None => continue,
        };

        if file.is_dir() {
            println!("File {} extracted to \"{}\"", i, out_path.display());
            if !dry_run {
                fs::create_dir_all(&out_path).unwrap();
            }
        } else {
            println!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                out_path.display(),
                file.size()
            );
            if let Some(p) = out_path.parent() {
                if !dry_run {
                    if !p.exists() {
                        fs::create_dir_all(p).unwrap();
                    }
                }
            }
            if !dry_run {
                let mut outfile = fs::File::create(&out_path).unwrap();
                io::copy(&mut file, &mut outfile).unwrap();
            }
        }
    }
}

pub(crate) fn count(s: &String) -> anyhow::Result<u32> {
    let fname = std::path::Path::new(s);
    let file = fs::File::open(fname)?;

    let mut archive = zip::ZipArchive::new(file)?;
    let mut count: u32 = 0;
    info!("Archive {} contains {} files", s, archive.len());
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => {
                // path;
                count += 1;
            }
            None => continue,
        };
    }
    Ok(count)
}
