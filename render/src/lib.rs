#[macro_use] extern crate log;
#[macro_use] extern crate pdf;

use pathfinder_geometry::rect::RectF;
use pathfinder_content::fill::FillRule;
mod cache;
mod fontentry;
mod graphicsstate;
mod renderstate;
mod textstate;

pub use cache::{Cache, ItemMap};

pub static STANDARD_FONTS: &[(&'static str, &'static str)] = &[
    ("Courier", "CourierStd.otf"),
    ("Courier-Bold", "CourierStd-Bold.otf"),
    ("Courier-Oblique", "CourierStd-Oblique.otf"),
    ("Courier-BoldOblique", "CourierStd-BoldOblique.otf"),
    
    ("Times-Roman", "MinionPro-Regular.otf"),
    ("Times-Bold", "MinionPro-Bold.otf"),
    ("Times-Italic", "MinionPro-It.otf"),
    ("Times-BoldItalic", "MinionPro-BoldIt.otf"),
    
    ("Helvetica", "MyriadPro-Regular.otf"),
    ("Helvetica-Bold", "MyriadPro-Bold.otf"),
    ("Helvetica-Oblique", "MyriadPro-It.otf"),
    ("Helvetica-BoldOblique", "MyriadPro-BoldIt.otf"),
    
    ("Symbol", "SY______.PFB"),
    ("ZapfDingbats", "AdobePiStd.otf"),
    
    ("Arial-BoldMT", "Arial-BoldMT.otf"),
    ("ArialMT", "ArialMT.ttf"),
    ("Arial-ItalicMT", "Arial-ItalicMT.otf"),
];

#[derive(Copy, Clone)]
pub struct BBox(Option<RectF>);
impl BBox {
    pub fn empty() -> Self {
        BBox(None)
    }
    pub fn add(&mut self, r2: RectF) {
        self.0 = Some(match self.0 {
            Some(r1) => r1.union_rect(r2),
            None => r2
        });
    }
    pub fn add_bbox(&mut self, bb: Self) {
        if let Some(r) = bb.0 {
            self.add(r);
        }
    }
    pub fn rect(self) -> Option<RectF> {
        self.0
    }
}
