#![feature(test)]
extern crate test;

use pdf::file::File as PdfFile;
use pdf::object::*;
use std::path::Path;
use pdf_render::Cache;
use pathfinder_renderer::scene::Scene;
use test::Bencher;

#[bench]
fn render_page(bencher: &mut Bencher) {
    let file = PdfFile::<Vec<u8>>::open("/home/sebk/Downloads/10.1016@j.eswa.2020.114101.pdf").unwrap();
    
    let page = file.get_page(1).unwrap();
    let mut cache = Cache::new();
    bencher.iter(|| cache.render_page(&file, &page, Default::default()).unwrap());
}
