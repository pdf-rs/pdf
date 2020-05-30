use pdf::file::File as PdfFile;
use pdf::object::*;
use pdf::error::PdfError;
use std::env;
use std::fs::File;
use std::io::BufWriter;
use pdf_render::Cache;
use pathfinder_export::{FileFormat, Export};

fn main() -> Result<(), PdfError> {
    env_logger::init();
    let mut args = env::args().skip(1);
    let path = args.next().expect("no file given");
    let first_page = args.next().map(|s| s.parse().expect("not a number")).unwrap_or(0);
    let last_page = args.next().map(|s| s.parse().expect("not a number")).unwrap_or(first_page);
    
    println!("read: {}", path);
    let file = PdfFile::<Vec<u8>>::open(&path)?;
    
    let mut cache = Cache::new();
    for (i, page) in file.pages().enumerate().skip(first_page).take(last_page + 1 - first_page) {
        println!("page {}", i);
        let p: &Page = &*page.unwrap();
        let (scene, _) = cache.render_page(&file, p)?;
        let mut writer = BufWriter::new(File::create(&format!("{}_{}.svg", path, i))?);
        scene.export(&mut writer, FileFormat::SVG)?;
    }
    Ok(())
}
