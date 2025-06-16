use std::{fs, io};
use tracing::info;

fn main(s: &str) {
    let fname = std::path::Path::new(s);
    let file = fs::File::open(fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let outpath = match file.enclosed_name() {
            Some(path) => path,
            None => continue,
        };

        {
            let comment = file.comment();
            if !comment.is_empty() {
                println!("File {i} comment: {comment}");
            }
        }

        if file.is_dir() {
            println!("File {} extracted to \"{}\"", i, outpath.display());
            fs::create_dir_all(&outpath).unwrap();
        } else {
            println!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.display(),
                file.size()
            );
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).unwrap();
                }
            }
            let mut outfile = fs::File::create(&outpath).unwrap();
            io::copy(&mut file, &mut outfile).unwrap();
        }
    }

}

pub(crate) fn count(s: &String) -> anyhow::Result<u32> {
    let fname = std::path::Path::new(s);
    let file = fs::File::open(fname)?;

    let mut archive = zip::ZipArchive::new(file)?;
    let mut count:u32 = 0;
    info!("Archive {} contains {} files", s, archive.len());
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => {
                // path;
                count += 1;
            },
            None => continue,
        };
    }
    Ok(count)
}