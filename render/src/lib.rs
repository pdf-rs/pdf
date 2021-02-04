#[macro_use] extern crate log;
#[macro_use] extern crate pdf;

use std::convert::TryInto;
use std::path::{PathBuf};
use std::collections::HashMap;
use std::fs;
use std::borrow::Cow;
use std::sync::Arc;

use pdf::file::File as PdfFile;
use pdf::object::*;
use pdf::primitive::Primitive;
use pdf::backend::Backend;
use pdf::font::{Font as PdfFont, Widths};
use pdf::error::{PdfError, Result};
use pdf::encoding::{BaseEncoding};
use pdf_encoding::{Encoding};

use pathfinder_geometry::{
    vector::{Vector2F, Vector2I},
    rect::RectF, transform2d::Transform2F,
};
use pathfinder_content::{
    fill::FillRule,
    stroke::{LineCap, LineJoin, StrokeStyle, OutlineStrokeToFill},
    outline::{Outline, Contour},
    pattern::{Pattern, Image},
};
use pathfinder_color::ColorU;
use pathfinder_renderer::{
    scene::{DrawPath, ClipPath, ClipPathId, Scene},
    paint::{Paint, PaintId},
};
use font::{self, Font, GlyphId};

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

macro_rules! ops_p {
    ($ops:ident, $($point:ident),* => $block:block) => ({
        let mut iter = $ops.iter();
        $(
            let x = iter.next().unwrap().as_number().unwrap();
            let y = iter.next().unwrap().as_number().unwrap();
            let $point = Vector2F::new(x, y);
        )*
        $block
    })
}
macro_rules! ops {
    ($ops:ident, $($var:ident : $typ:ty),* => $block:block) => ({
        let mut iter = $ops.iter();
        $(
            let $var: $typ = iter.next().ok_or(PdfError::EOF)?.try_into()?;
        )*
        $block
    })
}

fn rgb2fill(r: f32, g: f32, b: f32) -> Paint {
    let c = |v: f32| (v * 255.) as u8;
    Paint::from_color(ColorU::new(c(r), c(g), c(b), 255))
}
fn gray2fill(g: f32) -> Paint {
    rgb2fill(g, g, g)
}

fn cmyk2fill(c: f32, m: f32, y: f32, k: f32) -> Paint {
    let clamp = |f| if f > 1.0 { 1.0 } else { f };
    rgb2fill(
        1.0 - clamp(c + k),
        1.0 - clamp(m + k),
        1.0 - clamp(y + k),
    )
}

fn cmyk2color(data: &[u8]) -> Vec<ColorU> {
    data.chunks_exact(4).map(|c| {
        let mut buf = [0; 4];
        buf.copy_from_slice(c);

        let [c, m, y, k] = buf;
        let (c, m, y, k) = (255 - c, 255 - m, 255 - y, 255 - k);
        let r = 255 - c.saturating_add(k);
        let g = 255 - m.saturating_add(k);
        let b = 255 - y.saturating_add(k);
        ColorU::new(r, g, b, 255)
        
        /*
        let clamp = |f| if f > 1.0 { 1.0 } else { f };
        let i = |b| 1.0 - (b as f32 / 255.);
        let o = |f| (f * 255.) as u8;
        let (c, m, y, k) = (i(c), i(m), i(y), i(k));
        let (r, g, b) = (
            1.0 - clamp(c + k),
            1.0 - clamp(m + k),
            1.0 - clamp(y + k),
        );
        let (r, g, b) = (o(r), o(g), o(b));
        ColorU::new(r, g, b, 255)
        */
    }).collect()
}

#[derive(Copy, Clone)]
struct BBox(Option<RectF>);
impl BBox {
    fn empty() -> Self {
        BBox(None)
    }
    fn add(&mut self, r2: RectF) {
        self.0 = Some(match self.0 {
            Some(r1) => r1.union_rect(r2),
            None => r2
        });
    }
    fn add_bbox(&mut self, bb: Self) {
        if let Some(r) = bb.0 {
            self.add(r);
        }
    }
    fn rect(self) -> Option<RectF> {
        self.0
    }
}

#[derive(Debug)]
enum TextEncoding {
    CID,
    Cmap(HashMap<u16, GlyphId>)
}

struct FontEntry {
    font: Box<dyn Font>,
    encoding: TextEncoding,
    widths: Option<Widths>,
    is_cid: bool,
}
#[derive(Copy, Clone)]
enum TextMode {
    Fill,
    Stroke,
    FillThenStroke,
    Invisible,
    FillAndClip,
    StrokeAndClip
}

#[derive(Copy, Clone)]
struct GraphicsState<'a> {
    transform: Transform2F,
    stroke_style: StrokeStyle,
    fill_paint: PaintId,
    stroke_paint: PaintId,
    clip_path: Option<ClipPathId>,
    fill_color_space: &'a ColorSpace,
    stroke_color_space: &'a ColorSpace,
}

#[derive(Copy, Clone)]
enum DrawMode {
    Fill,
    Stroke,
    FillStroke,
    StrokeFill,
}

impl<'a> GraphicsState<'a> {
    fn draw(&self, scene: &mut Scene, outline: &Outline, mode: DrawMode, fill_rule: FillRule) {
        self.draw_transform(scene, outline, mode, fill_rule, Transform2F::default());
    }
    fn draw_transform(&self, scene: &mut Scene, outline: &Outline, mode: DrawMode, fill_rule: FillRule, transform: Transform2F) {
        let tr = self.transform * transform;
        let fill = |scene: &mut Scene| {
            let mut draw_path = DrawPath::new(outline.clone().transformed(&tr), self.fill_paint);
            draw_path.set_clip_path(self.clip_path);
            draw_path.set_fill_rule(fill_rule);
            scene.push_draw_path(draw_path);
        };

        if matches!(mode, DrawMode::Fill | DrawMode::FillStroke) {
            fill(scene);
        }
        if matches!(mode, DrawMode::Stroke | DrawMode::FillStroke) {
            let mut stroke = OutlineStrokeToFill::new(outline, self.stroke_style);
            stroke.offset();
            let mut draw_path = DrawPath::new(stroke.into_outline().transformed(&tr), self.stroke_paint);
            draw_path.set_clip_path(self.clip_path);
            draw_path.set_fill_rule(fill_rule);
            scene.push_draw_path(draw_path);
        }
        if matches!(mode, DrawMode::StrokeFill) {
            fill(scene);
        }
    }
}

#[derive(Copy, Clone)]
struct TextState<'a> {
    text_matrix: Transform2F, // tracks current glyph
    line_matrix: Transform2F, // tracks current line
    char_space: f32, // Character spacing
    word_space: f32, // Word spacing
    horiz_scale: f32, // Horizontal scaling
    leading: f32, // Leading
    font_entry: Option<&'a FontEntry>, // Text font
    font_size: f32, // Text font size
    mode: TextMode, // Text rendering mode
    rise: f32, // Text rise
    knockout: f32, //Text knockout
}
impl<'a> TextState<'a> {
    fn new() -> TextState<'a> {
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
    fn reset_matrix(&mut self) {
        self.set_matrix(Transform2F::default());
    }
    fn translate(&mut self, v: Vector2F) {
        let m = self.line_matrix * Transform2F::from_translation(v);
        self.set_matrix(m);
    }
    
    // move to the next line
    fn next_line(&mut self) {
        self.translate(Vector2F::new(0., -self.leading));
    }
    // set text and line matrix
    fn set_matrix(&mut self, m: Transform2F) {
        self.text_matrix = m;
        self.line_matrix = m;
    }
    fn add_glyphs(&mut self, scene: &mut Scene, gs: &GraphicsState, glyphs: impl Iterator<Item=(u16, Option<GlyphId>, bool)>) -> BBox {
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

            debug!("cid {} -> gid {:?}", cid, gid);
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
                    bbox.add(path.bounds());
                    gs.draw_transform(scene, &path, draw_mode, FillRule::Winding, transform);
                }
            } else {
                info!("no glyph for gid {:?}", gid);
            }
            let advance = self.char_space * self.horiz_scale * self.font_size + width;
            self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
        }
        debug!("text: {}", text);
        bbox
    }
    fn draw_text(&mut self, scene: &mut Scene, gs: &GraphicsState, data: &[u8]) -> BBox {
        debug!("text: {:?}", String::from_utf8_lossy(data));
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
    fn advance(&mut self, delta: f32) {
        //debug!("advance by {}", delta);
        let advance = delta * self.font_size * self.horiz_scale;
        self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
    }
}

pub struct Cache {
    // shared mapping of fontname -> font
    fonts: HashMap<String, FontEntry>
}
impl FontEntry {
    fn build(font: Box<dyn Font>, pdf_font: &PdfFont) -> FontEntry {
        let mut is_cid = pdf_font.is_cid();
        let encoding = pdf_font.encoding().clone();
        let base_encoding = encoding.as_ref().map(|e| &e.base);

        let encoding = if let Some(map) = pdf_font.cid_to_gid_map() {
            is_cid = true;
            let cmap = map.iter().enumerate().map(|(cid, &gid)| (cid as u16, GlyphId(gid as u32))).collect();
            TextEncoding::Cmap(cmap)
        } else if base_encoding == Some(&BaseEncoding::IdentityH) {
            is_cid = true;
            TextEncoding::CID
        } else {
            let mut cmap = HashMap::new();
            let source_encoding = match base_encoding {
                Some(BaseEncoding::StandardEncoding) => Some(Encoding::AdobeStandard),
                Some(BaseEncoding::SymbolEncoding) => Some(Encoding::AdobeSymbol),
                Some(BaseEncoding::WinAnsiEncoding) => Some(Encoding::WinAnsiEncoding),
                ref e => {
                    warn!("unsupported pdf encoding {:?}", e);
                    None
                }
            };
            let font_encoding = font.encoding();
            debug!("{:?} -> {:?}", source_encoding, font_encoding);
            match (source_encoding, font_encoding) {
                (Some(source), Some(dest)) => {
                    let transcoder = source.to(dest).expect("can't transcode");
                    
                    for b in 0 .. 256 {
                        if let Some(gid) = transcoder.translate(b).and_then(|cp| font.gid_for_codepoint(cp)) {
                            cmap.insert(b as u16, gid);
                            debug!("{} -> {:?}", b, gid);
                        }
                    }
                },
                _ => {
                    warn!("can't translate from text encoding {:?} to font encoding {:?}", base_encoding, font_encoding);
                    
                    // assuming same encoding
                    for cp in 0 .. 256 {
                        if let Some(gid) = font.gid_for_codepoint(cp) {
                            cmap.insert(cp as u16, gid);
                        }
                    }
                }
            }
            if let Some(encoding) = encoding {
                for (&cp, name) in encoding.differences.iter() {
                    debug!("{} -> {}", cp, name);
                    match font.gid_for_name(&name) {
                        Some(gid) => {
                            cmap.insert(cp as u16, gid);
                        }
                        None => info!("no glyph for name {}", name)
                    }
                }
            }
            debug!("cmap: {:?}", cmap);
            if cmap.is_empty() {
                TextEncoding::CID
            } else {
                TextEncoding::Cmap(cmap)
            }
        };
        
        let widths = pdf_font.widths().unwrap();

        FontEntry {
            font: font,
            encoding,
            is_cid,
            widths,
        }
    }
}

#[derive(Debug)]
pub struct ItemMap(Vec<(RectF, Box<dyn std::fmt::Debug>)>);
impl ItemMap {
    pub fn print(&self, p: Vector2F) {
        for &(rect, ref op) in self.0.iter() {
            if rect.contains_point(p) {
                println!("{:?}", op);
            }
        }
    }
    pub fn get_string(&self, p: Vector2F) -> Option<String> {
        use itertools::Itertools;
        let mut iter = self.0.iter().filter_map(|&(rect, ref op)| {
            if rect.contains_point(p) {
                Some(op)
            } else {
                None
            }
        }).peekable();
        if iter.peek().is_some() {
            Some(format!("{:?}", iter.format(", ")))
        } else {
            None
        }
    }
    fn new() -> Self {
        ItemMap(Vec::new())
    }
    fn add_rect(&mut self, rect: RectF, item: impl std::fmt::Debug + 'static) {
        self.0.push((rect, Box::new(item) as _));
    }
    fn add_bbox(&mut self, bbox: BBox, item: impl std::fmt::Debug + 'static) {
        if let Some(r) = bbox.rect() {
            self.add_rect(r, item);
        }
    }
}

fn fill_rule(s: &str) -> FillRule {
    if s.ends_with("*") {
        FillRule::EvenOdd
    } else {
        FillRule::Winding
    }
}

fn convert_color(cs: &ColorSpace, ops: &[Primitive]) -> Result<Paint> {
    match *cs {
        ColorSpace::DeviceRGB | ColorSpace::Icc(_) => ops!(ops, r: f32, g: f32, b: f32 => {
            Ok(rgb2fill(r, g, b))
        }),
        ColorSpace::DeviceCMYK => ops!(ops, c: f32, m: f32, y: f32, k: f32 => {
            Ok(cmyk2fill(c, m, y, k))
        }),
        ColorSpace::Separation(ref name, ref alt, ref f) => ops!(ops, x: f32 => {
            match &**alt {
                &ColorSpace::DeviceCMYK => {
                    let mut cmyk = [0.0; 4];
                    f.apply(x, &mut cmyk);
                    let [c, m, y, k] = cmyk;
                    Ok(cmyk2fill(c, m, y, k))
                },
                &ColorSpace::DeviceRGB => {
                    let mut rgb = [0.0, 0.0, 0.0];
                    f.apply(x, &mut rgb);
                    let [r, g, b] = rgb;
                    Ok(rgb2fill(r, g, b))
                },
                c => unimplemented!("{:?}", c)
            }
        }),
        ColorSpace::Indexed(ref cs, ref lut) => ops!(ops, i: i32 => {
            match **cs {
                ColorSpace::DeviceRGB => {
                    let c = &lut[3 * i as usize ..];
                    Ok(Paint::from_color(ColorU::new(c[0], c[1], c[2], 255)))
                }
                ColorSpace::DeviceCMYK => {
                    let c = &lut[4 * i as usize ..];
                    let cvt = |b: u8| b as f32 * 255.;
                    Ok(cmyk2fill(cvt(c[0]), cvt(c[1]), cvt(c[2]), cvt(c[3])))
                }
                _ => unimplemented!()
            }
        }),
        _ => unimplemented!()
    }
}

impl Cache {
    pub fn new() -> Cache {
        Cache {
            fonts: HashMap::new()
        }
    }
    fn load_font(&mut self, pdf_font: &PdfFont) {
        if self.fonts.get(&pdf_font.name).is_some() {
            return;
        }
        
        debug!("loading {:?}", pdf_font);
        
        let data: Cow<[u8]> = match pdf_font.embedded_data() {
            Some(Ok(data)) => {
                if let Some(path) = std::env::var_os("PDF_FONTS") {
                    let file = PathBuf::from(path).join(&pdf_font.name);
                    fs::write(file, data).expect("can't write font");
                }
                data.into()
            }
            Some(Err(e)) => panic!("can't decode font data: {:?}", e),
            None => {
                match STANDARD_FONTS.iter().find(|&&(name, _)| pdf_font.name == name) {
                    Some(&(_, file_name)) => {
                        if let Ok(data) = std::fs::read(file_name) {
                            data.into()
                        } else {
                            warn!("can't open {} for {}", file_name, pdf_font.name);
                            return;
                        }
                    }
                    None => {
                        warn!("no font for {}", pdf_font.name);
                        return;
                    }
                }
            }
        };
        let entry = FontEntry::build(font::parse(&data), pdf_font);
        debug!("is_cid={}", entry.is_cid);
            
        self.fonts.insert(pdf_font.name.clone(), entry);
    }
    fn get_font(&self, font_name: &str) -> Option<&FontEntry> {
        self.fonts.get(font_name)
    }
    
    pub fn render_page<B: Backend>(&mut self, file: &PdfFile<B>, page: &Page, transform: Transform2F) -> Result<(Scene, ItemMap)> {
        let Rect { left, right, top, bottom } = page.media_box(file).expect("no media box");
        let rect = RectF::from_points(Vector2F::new(left, bottom), Vector2F::new(right, top));
        
        let scale = 25.4 / 72.;
        let mut scene = Scene::new();
        let view_box = transform * RectF::new(Vector2F::default(), rect.size() * scale);
        scene.set_view_box(view_box);
        
        let black = scene.push_paint(&Paint::from_color(ColorU::black()));
        let white = scene.push_paint(&Paint::from_color(ColorU::white()));

        scene.push_draw_path(DrawPath::new(Outline::from_rect(view_box), white));

        let mut items = ItemMap::new();

        let root_transformation = transform * Transform2F::from_scale(scale) * Transform2F::row_major(1.0, 0.0, -left, 0.0, -1.0, top);
        
        let resources = page.resources(file)?;
        // make sure all fonts are in the cache, so we can reference them
        for font in resources.fonts.values() {
            self.load_font(font);
        }
        for gs in resources.graphics_states.values() {
            if let Some((ref font, _)) = gs.font {
                self.load_font(font);
            }
        }

        let device_rgb = ColorSpace::DeviceRGB;
        
        let mut text_state = TextState::new();
        let mut stack = vec![];
        let mut current_outline = Outline::new();
        let mut current_contour = Contour::new();

        fn flush(outline: &mut Outline, contour: &mut Contour) {
            if !contour.is_empty() {
                outline.push_contour(contour.clone());
                contour.clear();
            }
        }

        let mut graphics_state = GraphicsState {
            transform: root_transformation,
            fill_paint: black,
            stroke_paint: black,
            clip_path: None,
            fill_color_space: &device_rgb,
            stroke_color_space: &device_rgb,
            stroke_style: StrokeStyle {
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter(1.0),
                line_width: 1.0,
            }
        };
        
        let contents = try_opt!(page.contents.as_ref());
        
        for op in contents.operations.iter() {
            debug!("{}", op);
            let ref ops = op.operands;
            let s = op.operator.as_str();
            match s {
                "m" => { // move x y
                    ops_p!(ops, p => {
                        flush(&mut current_outline, &mut current_contour);
                        current_contour.push_endpoint(p);
                    })
                }
                "l" => { // line x y
                    ops_p!(ops, p => {
                        current_contour.push_endpoint(p);
                    })
                }
                "c" => { // cubic bezier c1.x c1.y c2.x c2.y p.x p.y
                    ops_p!(ops, c1, c2, p => {
                        current_contour.push_cubic(c1, c2, p);
                    })
                }
                "v" => { // cubic bezier c2.x c2.y p.x p.y
                    ops_p!(ops, c2, p => {
                        let c1 = current_contour.last_position().unwrap_or_default();
                        current_contour.push_cubic(c1, c2, p);
                    })
                }
                "y" => { // cubic c1.x c1.y p.x p.y
                    ops_p!(ops, c1, p => {
                        current_contour.push_cubic(c1, p, p);
                    })
                }
                "h" => { // close
                    current_contour.close();
                }
                "re" => { // rect x y width height
                    ops_p!(ops, origin, size => {
                        flush(&mut current_outline, &mut current_contour);
                        current_outline.push_contour(Contour::from_rect(RectF::new(origin, size)));
                    })
                }
                "S" => { // stroke
                    flush(&mut current_outline, &mut current_contour);
                    graphics_state.draw(&mut scene, &current_outline, DrawMode::Stroke, FillRule::Winding);
                    current_outline.clear();
                }
                "s" => { // close and stroke
                    current_contour.close();
                    flush(&mut current_outline, &mut current_contour);
                    graphics_state.draw(&mut scene, &current_outline, DrawMode::Stroke, FillRule::Winding);
                    current_outline.clear();
                }
                "f" | "F" => { // close and fill 
                    current_contour.close();
                    flush(&mut current_outline, &mut current_contour);
                    graphics_state.draw(&mut scene, &current_outline, DrawMode::Fill, fill_rule(s));
                    current_outline.clear();
                }
                "B" | "B*" => { // fill and stroke
                    flush(&mut current_outline, &mut current_contour);
                    graphics_state.draw(&mut scene, &current_outline, DrawMode::FillStroke, fill_rule(s));
                    current_outline.clear();
                }
                "b" | "b*" => { // close, stroke and fill
                    current_contour.close();
                    flush(&mut current_outline, &mut current_contour);
                    graphics_state.draw(&mut scene, &current_outline, DrawMode::FillStroke, fill_rule(s));
                    current_outline.clear();
                }
                "n" => { // clear path
                    current_outline.clear();
                    current_contour.clear();
                }
                "W" | "W*" => {
                    flush(&mut current_outline, &mut current_contour);
                    let path = current_outline.clone().transformed(&graphics_state.transform);
                    let mut clip_path = ClipPath::new(path);
                    clip_path.set_fill_rule(fill_rule(s));
                    let clip_path_id = scene.push_clip_path(clip_path);
                    graphics_state.clip_path = Some(clip_path_id);
                }
                "q" => { // save state
                    stack.push((graphics_state.clone(), text_state));
                }
                "Q" => { // restore
                    let (g, t) = stack.pop().expect("graphcs stack is empty");
                    graphics_state = g;
                    text_state = t;
                }
                "cm" => { // modify transformation matrix 
                    ops!(ops, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 => {
                        graphics_state.transform = graphics_state.transform * Transform2F::row_major(a, c, e, b, d, f);
                    })
                }
                "w" => { // line width
                    ops!(ops, width: f32 => {
                        graphics_state.stroke_style.line_width = width;
                    })
                }
                "J" => { // line cap
                }
                "j" => { // line join 
                }
                "M" => { // miter limit
                }
                "d" => { // line dash [ array phase ]
                }
                "gs" => ops!(ops, gs: &Primitive => { // set from graphic state dictionary
                    let gs = gs.as_name()?;
                    let gs = try_opt!(resources.graphics_states.get(gs));
                    if let Some(lw) = gs.line_width {
                        graphics_state.stroke_style.line_width = lw;
                    }
                    if let Some((ref font, size)) = gs.font {
                        if let Some(e) = self.get_font(&font.name) {
                            text_state.font_entry = Some(e);
                            text_state.font_size = size;
                            debug!("new font: {} at size {}", font.name, size);
                        } else {
                            text_state.font_entry = None;
                        }
                    }
                }),
                "SC" | "SCN" | "RG" => { // stroke color
                    let paint = convert_color(graphics_state.stroke_color_space, &*ops)?;
                    graphics_state.stroke_paint = scene.push_paint(&paint);
                }
                "sc" | "scn" | "rg" => { // fill color
                    let paint = convert_color(graphics_state.fill_color_space, &*ops)?;
                    graphics_state.fill_paint = scene.push_paint(&paint);
                }
                "G" => { // stroke gray
                    ops!(ops, gray: f32 => {
                        graphics_state.stroke_paint = scene.push_paint(&gray2fill(gray));
                    })
                }
                "g" => { // fill gray
                    ops!(ops, gray: f32 => {
                        graphics_state.fill_paint = scene.push_paint(&gray2fill(gray));
                    })
                }
                "K" => { // stroke color
                    ops!(ops, c: f32, m: f32, y: f32, k: f32 => {
                        graphics_state.stroke_paint = scene.push_paint(&cmyk2fill(c, m, y, k));
                    });
                }
                "k" => { // fill color
                    ops!(ops, c: f32, m: f32, y: f32, k: f32 => {
                        graphics_state.fill_paint = scene.push_paint(&cmyk2fill(c, m, y, k));
                    });
                }
                "cs" => { // color space
                    ops!(ops, name: &Primitive => {
                        let name = name.as_name()?;
                        graphics_state.fill_color_space = resources.color_spaces.get(name).unwrap().clone();
                    });
                }
                "CS" => { // color space
                    ops!(ops, name: &Primitive => {
                        let name = name.as_name()?;
                        graphics_state.stroke_color_space = resources.color_spaces.get(name).unwrap().clone();
                    });
                }
                "BT" => {
                    text_state.reset_matrix();
                }
                "ET" => {
                }
                // state modifiers
                
                // character spacing
                "Tc" => ops!(ops, char_space: f32 => {
                    text_state.char_space = char_space;
                }),
                
                // word spacing
                "Tw" => ops!(ops, word_space: f32 => {
                    text_state.word_space = word_space;
                }),
                
                // Horizontal scaling (in percent)
                "Tz" => ops!(ops, scale: f32 => {
                    text_state.horiz_scale = 0.01 * scale;
                }),
                
                // leading
                "TL" => ops!(ops, leading: f32 => {
                    text_state.leading = leading;
                }),
                
                // text font
                "Tf" => ops!(ops, font_name: &Primitive, size: f32 => {
                    let font_name = font_name.as_name()?;
                    let font = try_opt!(resources.fonts.get(font_name));
                    if let Some(e) = self.get_font(&font.name) {
                        text_state.font_entry = Some(e);
                        debug!("new font: {} (is_cid={:?})", font.name, e.is_cid);
                        text_state.font_size = size;
                    } else {
                        text_state.font_entry = None;
                    }
                }),
                
                // render mode
                "Tr" => ops!(ops, mode: i32 => {
                    use TextMode::*;
                    text_state.mode = match mode {
                        0 => Fill,
                        1 => Stroke,
                        2 => FillThenStroke,
                        3 => Invisible,
                        4 => FillAndClip,
                        5 => StrokeAndClip,
                        _ => {
                            return Err(PdfError::Other { msg: format!("Invalid text render mode: {}", mode)});
                        }
                    }
                }),
                
                // text rise
                "Ts" => ops!(ops, rise: f32 => {
                    text_state.rise = rise;
                }),
                
                // positioning operators
                // Move to the start of the next line
                "Td" => ops_p!(ops, t => {
                    text_state.translate(t);
                }),
                
                "TD" => ops_p!(ops, t => {
                    text_state.leading = -t.y();
                    text_state.translate(t);
                }),
                
                // Set the text matrix and the text line matrix
                "Tm" => ops!(ops, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 => {
                    text_state.set_matrix(Transform2F::row_major(a, c, e, b, d, f));
                }),
                
                // Move to the start of the next line
                "T*" => {
                    text_state.next_line();
                },
                
                // draw text
                "Tj" => ops!(ops, text: &[u8] => {
                    let bb = text_state.draw_text(&mut scene, &graphics_state, text);
                    items.add_bbox(bb, op.clone());
                }),
                
                // move to the next line and draw text
                "'" => ops!(ops, text: &[u8] => {
                    text_state.next_line();
                    let bb = text_state.draw_text(&mut scene, &graphics_state, text);
                    items.add_bbox(bb, op.clone());
                }),
                
                // set word and charactr spacing, move to the next line and draw text
                "\"" => ops!(ops, word_space: f32, char_space: f32, text: &[u8] => {
                    text_state.word_space = word_space;
                    text_state.char_space = char_space;
                    text_state.next_line();
                    let bb = text_state.draw_text(&mut scene, &graphics_state, text);
                    items.add_bbox(bb, op.clone());
                }),
                "TJ" => ops!(ops, array: &[Primitive] => {
                    let mut bb = BBox::empty();
                    for arg in array {
                        match arg {
                            Primitive::String(ref data) => {
                                let r2 = text_state.draw_text(&mut scene, &graphics_state, data.as_bytes());
                                bb.add_bbox(r2);
                            },
                            p => {
                                let offset = p.as_number().expect("wrong argument to TJ");
                                text_state.advance(-0.001 * offset); // because why not PDF…
                            }
                        }
                    }
                    items.add_bbox(bb, op.clone());
                }),
                "Do" => ops!(ops, name: &Primitive => {
                    let mut closure = || -> Result<()> {
                        let name = name.as_name()?;
                        let &xobject_ref = resources.xobjects.get(name).unwrap();
                        let xobject = file.get(xobject_ref)?;
                        match *xobject {
                            XObject::Image(ref image) => {
                                let raw_data = image.data()?;
                                let pixel_count = image.width as usize * image.height as usize;
                                if raw_data.len() % pixel_count != 0 {
                                    warn!("invalid data length {} bytes for {} pixels", raw_data.len(), pixel_count);
                                    return Err(PdfError::EOF);
                                }
                                let data = match raw_data.len() / pixel_count {
                                    1 => raw_data.iter().map(|&l| ColorU { r: l, g: l, b: l, a: 255 }).collect(),
                                    3 => raw_data.chunks_exact(3).map(|c| ColorU { r: c[0], g: c[1], b: c[2], a: 255 }).collect(),
                                    4 => cmyk2color(raw_data),
                                    n => panic!("unimplemented {} bytes/pixel", n)
                                };
                                let size = Vector2I::new(image.width as _, image.height as _);
                                let size_f = size.to_f32();
                                let outline = Outline::from_rect(graphics_state.transform * RectF::new(Vector2F::default(), Vector2F::new(1.0, 1.0)));
                                let im_tr = graphics_state.transform
                                    * Transform2F::from_scale(Vector2F::new(1.0 / size_f.x(), -1.0 / size_f.y()))
                                    * Transform2F::from_translation(Vector2F::new(0.0, -size_f.y()));
                                let mut pattern = Pattern::from_image(Image::new(size, Arc::new(data)));
                                pattern.apply_transform(im_tr);
                                let paint = Paint::from_pattern(pattern);
                                let paint_id = scene.push_paint(&paint);
                                let mut draw_path = DrawPath::new(outline, paint_id);
                                draw_path.set_clip_path(graphics_state.clip_path);
                                scene.push_draw_path(draw_path);

                                items.add_rect(graphics_state.transform * RectF::new(Vector2F::default(), size_f), image.clone())
                            },
                            _ => {}
                        }
                        Ok(())
                    };
                    match closure() {
                        Ok(()) => {},
                        Err(e) => warn!("failed to decode image: {}", e)
                    }
                }),
                _ => {}
            }
        }
        
        Ok((scene, items))
    }
}
