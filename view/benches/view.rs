#![feature(test)]
extern crate test;

use pathfinder_renderer::scene::Scene;
use pdf::file::File as PdfFile;
use pdf::object::*;
use std::path::Path;
use test::Bencher;
use view::Cache;

fn render_file(path: &Path) -> Vec<Scene> {
    let file = PdfFile::<Vec<u8>>::open(path).unwrap();

    let mut cache = Cache::new();
    file.pages()
        .map(|page| {
            let p: &Page = &*page.unwrap();
            cache.render_page(&file, p).unwrap()
        })
        .collect()
}

macro_rules! bench_file {
    ($file:expr, $name:ident) => {
        #[bench]
        fn $name(bencher: &mut Bencher) {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("files")
                .join($file);
            bencher.iter(|| render_file(&path))
        }
    };
}

bench_file!("example.pdf", example);
bench_file!("ep.pdf", ep);
bench_file!("ep2.pdf", ep2);
bench_file!("libreoffice.pdf", libreoffice);
bench_file!("pdf-sample.pdf", pdf_sample);
bench_file!("xelatex-drawboard.pdf", xelatex_drawboard);
bench_file!("xelatex.pdf", xelatex);
bench_file!("PDF32000_2008.pdf", pdf32000);
