use pdf_view::PdfView;
use pdf::file::File;
use pathfinder_view::{show, Config};

fn main() {
    env_logger::init();
    let path = std::env::args().nth(1).unwrap();
    let file = File::<Vec<u8>>::open(&path).unwrap();
    let view = PdfView::new(file);
    show(view, Config { zoom: true, pan: true });
}
