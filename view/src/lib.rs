#[macro_use] extern crate log;

use pdf::file::File as PdfFile;
use pdf::backend::Backend;
use pdf::error::PdfError;
use pdf_render::Cache;

use pathfinder_view::{Interactive};
use pathfinder_renderer::scene::Scene;
use winit::event::{ElementState, VirtualKeyCode};

pub struct PdfView<B: Backend> {
    file: PdfFile<B>,
    current_page: u32,
    cache: Cache<Scene>,
}
impl<B: Backend> PdfView<B> {
    pub fn new(file: PdfFile<B>) -> Self {
        PdfView {
            file,
            current_page: 0,
            cache: Cache::new()
        }
    }
}
impl<B: Backend + 'static> Interactive for PdfView<B> {
    fn scene(&mut self) -> Scene {
        let page = self.file.get_page(self.current_page).unwrap();
        let scene = self.cache.render_page(&self.file, &page).unwrap();
        scene
    }
    fn keyboard_input(&mut self, state: ElementState, keycode: VirtualKeyCode) -> bool {
        match (state, keycode) {
            (ElementState::Pressed, VirtualKeyCode::Left) if self.current_page > 0 => {
                self.current_page -= 1;
                true
            }
            (ElementState::Pressed, VirtualKeyCode::Right) if self.current_page < self.file.num_pages() - 1 => {
                self.current_page += 1;
                true
            }
            _ => false
        }
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info);
    warn!("test");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn show(data: &Uint8Array) {
    let data: Vec<u8> = data.to_vec();
    info!("got {} bytes of data", data.len());
    let file = PdfFile::from_data(data).expect("failed to parse PDF");
    info!("got the file");
    let view = PdfView::new(file);

    info!("showing");
    pathfinder_view::show(view);
}
