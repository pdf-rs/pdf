use pdf::file::File as PdfFile;
use pdf::object::*;
use std::path::Path;
use std::env::args_os;
use std::panic::catch_unwind;
use pdf_render::Cache;
use pathfinder_renderer::scene::Scene;

fn render_file(path: &Path) -> Vec<Scene> {
    let file = PdfFile::<Vec<u8>>::open(path).unwrap();
    
    let mut cache = Cache::new();
    file.pages().map(|page| {
        let p: &Page = &*page.unwrap();
        cache.render_page(&file, p).unwrap().0
    }).collect()
}

fn main() {
    env_logger::init();
    for file in args_os().skip(1) {
        println!("{}", file.to_str().unwrap());
        match catch_unwind(|| render_file(Path::new(&file))) {
            Ok(_) => println!("... OK"),
            Err(_) => println!("... panicked")
        }
    }
}
