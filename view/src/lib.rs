#[macro_use] extern crate log;

use pdf::file::File as PdfFile;
use pdf::backend::Backend;
use pdf::error::PdfError;
use pdf_render::{Cache, ItemMap};

use pathfinder_view::{*};
use pathfinder_renderer::scene::Scene;
use pathfinder_geometry::vector::Vector2F;

pub struct PdfView<B: Backend> {
    file: PdfFile<B>,
    num_pages: usize,
    cache: Cache,
    map: Option<ItemMap>,
}
impl<B: Backend> PdfView<B> {
    pub fn new(file: PdfFile<B>) -> Self {
        PdfView {
            num_pages: file.num_pages() as usize,
            file,
            cache: Cache::new(),
            map: None,
        }
    }
}
impl<B: Backend + 'static> Interactive for PdfView<B> {
    fn title(&self) -> String {
        self.file.trailer.info_dict.as_ref()
            .and_then(|info| info.get("Title"))
            .and_then(|p| p.as_str().map(|s| s.into_owned()))
            .unwrap_or_else(|| "PDF View".into())
    }
    fn num_pages(&self) -> usize {
        self.num_pages
    }
    fn scene(&mut self, page_nr: usize) -> Scene {
        let page = self.file.get_page(page_nr as u32).unwrap();
        let (scene, map) = self.cache.render_page(&self.file, &page).unwrap();
        self.map = Some(map);
        scene
    }
    fn mouse_input(&mut self, ctx: &mut Context, page: usize, pos: Vector2F, state: ElementState) {
        if state != ElementState::Pressed { return; }
        info!("x={}, y={}", pos.x(), pos.y());

        if let Some(ref map) = self.map {
            if let Some(text) = map.get_string(pos) {
                info!("{}", text);
            }
        }
    }
    fn keyboard_input(&mut self, ctx: &mut Context, event: &mut KeyEvent) {
        if event.state == ElementState::Released {
            return;
        }
        match event.keycode {
            KeyCode::Right | KeyCode::PageDown => ctx.next_page(),
            KeyCode::Left | KeyCode::PageUp => ctx.prev_page(),
            _ => return
        }
        ctx.update_scene();
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;

#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlCanvasElement};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info);
    warn!("test");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn show(canvas: HtmlCanvasElement, data: &Uint8Array) -> WasmView {
    let data: Vec<u8> = data.to_vec();
    info!("got {} bytes of data", data.len());
    let file = PdfFile::from_data(data).expect("failed to parse PDF");
    info!("got the file");
    let view = PdfView::new(file);

    WasmView::new(
        canvas,
        Config { zoom: false, pan: false },
        Box::new(view) as _
    )
}
