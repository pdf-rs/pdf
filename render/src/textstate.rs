use pathfinder_geometry::{
    vector::Vector2F,
    transform2d::Transform2F,
};
use pathfinder_renderer::scene::Scene;
use pathfinder_content::{
    fill::FillRule,
};
use font::GlyphId;
use super::{
    BBox,
    fontentry::{FontEntry, TextEncoding},
    graphicsstate::{GraphicsState, DrawMode},
};
use std::convert::TryInto;

#[derive(Copy, Clone)]
pub enum TextMode {
    Fill,
    Stroke,
    FillThenStroke,
    Invisible,
    FillAndClip,
    StrokeAndClip
}


#[derive(Copy, Clone)]
pub struct TextState<'a> {
    pub text_matrix: Transform2F, // tracks current glyph
    pub line_matrix: Transform2F, // tracks current line
    pub char_space: f32, // Character spacing
    pub word_space: f32, // Word spacing
    pub horiz_scale: f32, // Horizontal scaling
    pub leading: f32, // Leading
    pub font_entry: Option<&'a FontEntry>, // Text font
    pub font_size: f32, // Text font size
    pub mode: TextMode, // Text rendering mode
    pub rise: f32, // Text rise
    pub knockout: f32, //Text knockout
}
impl<'a> TextState<'a> {
    pub fn new() -> TextState<'a> {
        TextState {
            text_matrix: Transform2F::default(),
            line_matrix: Transform2F::default(),
            char_space: 0.,
            word_space: 0.,
            horiz_scale: 1.,
            leading: 0.,
            font_entry: None,
            font_size: 0.,
            mode: TextMode::Fill,
            rise: 0.,
            knockout: 0.
        }
    }
    pub fn reset_matrix(&mut self) {
        self.set_matrix(Transform2F::default());
    }
    pub fn translate(&mut self, v: Vector2F) {
        let m = self.line_matrix * Transform2F::from_translation(v);
        self.set_matrix(m);
    }
    
    // move to the next line
    pub fn next_line(&mut self) {
        self.translate(Vector2F::new(0., -self.leading));
    }
    // set text and line matrix
    pub fn set_matrix(&mut self, m: Transform2F) {
        self.text_matrix = m;
        self.line_matrix = m;
    }
    pub fn add_glyphs(&mut self, scene: &mut Scene, gs: &GraphicsState, glyphs: impl Iterator<Item=(u16, Option<GlyphId>, bool)>) -> BBox {
        let draw_mode = match self.mode {
            TextMode::Fill => DrawMode::Fill,
            TextMode::FillAndClip => DrawMode::Fill,
            TextMode::FillThenStroke => DrawMode::FillStroke,
            TextMode::Invisible => return BBox::empty(),
            TextMode::Stroke => DrawMode::Stroke,
            TextMode::StrokeAndClip => DrawMode::Stroke
        };
        let e = self.font_entry.as_ref().expect("no font");
        let mut bbox = BBox::empty();

        let tr = Transform2F::row_major(
            self.horiz_scale * self.font_size, 0., 0.,
            0., self.font_size, self.rise
        ) * e.font.font_matrix();
        
        let mut text = String::with_capacity(32);
        for (cid, gid, is_space) in glyphs {
            if let Some(c) = std::char::from_u32(cid as u32) {
                text.push(c);
            }

            //debug!("cid {} -> gid {:?}", cid, gid);
            let gid = match gid {
                Some(gid) => gid,
                None => {
                    warn!("no glyph for cid {}", cid);
                    GlyphId(0)
                } // lets hope that works…
            };
            let glyph = e.font.glyph(gid);
            let width: f32 = e.widths.as_ref().map(|w| w.get(cid as usize) * 0.001 * self.horiz_scale * self.font_size)
                .or_else(|| glyph.as_ref().map(|g| tr.m11() * g.metrics.advance))
                .unwrap_or(0.0);
            
            if is_space {
                let advance = self.word_space * self.horiz_scale * self.font_size + width;
                self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
                continue;
            }
            if let Some(glyph) = glyph {
                let transform = self.text_matrix * tr;
                let path = glyph.path;
                if path.len() != 0 {
                    bbox.add(gs.transform * transform * path.bounds());
                    gs.draw_transform(scene, &path, draw_mode, FillRule::Winding, transform);
                }
            } else {
                info!("no glyph for gid {:?}", gid);
            }
            let advance = self.char_space * self.horiz_scale * self.font_size + width;
            self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
        }
        bbox
    }
    pub fn draw_text(&mut self, scene: &mut Scene, gs: &GraphicsState, data: &[u8]) -> BBox {
        if let Some(e) = self.font_entry {
            let get_glyph = |cid: u16| {
                let (gid, is_space) = match e.encoding {
                    TextEncoding::CID => (Some(GlyphId(cid as u32)), false),
                    TextEncoding::Cmap(ref cmap) => (cmap.get(&cid).cloned(), cid == 0x20),
                };
                (cid, gid, is_space)
            };
            if e.is_cid {
                self.add_glyphs(scene, gs,
                    data.chunks_exact(2).map(|s| get_glyph(u16::from_be_bytes(s.try_into().unwrap()))),
                )
            } else {
                self.add_glyphs(scene, gs,
                    data.iter().map(|&b| get_glyph(b as u16)),
                )
            }
        } else {
            warn!("no font set");
            BBox::empty()
        }
    }
    pub fn advance(&mut self, delta: f32) {
        //debug!("advance by {}", delta);
        let advance = delta * self.font_size * self.horiz_scale;
        self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
    }
}
