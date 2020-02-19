#[macro_use] extern crate log;
#[macro_use] extern crate pdf;

use std::convert::TryInto;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::fs;
use std::borrow::Cow;

use pdf::file::File as PdfFile;
use pdf::object::*;
use pdf::primitive::Primitive;
use pdf::backend::Backend;
use pdf::font::{Font as PdfFont, FontType};
use pdf::error::{PdfError, Result};
use pdf::encoding::{Encoding as PdfEncoding, BaseEncoding};
use encoding::{Encoding};

use pathfinder_geometry::{
    vector::Vector2F, rect::RectF, transform2d::Transform2F,
};
use font::{self, Font, GlyphId};
use vector::{Surface, Rgba8, PathStyle, PathBuilder, Outline, FillRule};

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
        $block;
    })
}

fn rgb2fill(r: f32, g: f32, b: f32) -> Rgba8 {
    let c = |v: f32| (v * 255.) as u8;
    (c(r), c(g), c(b), 255)
}
fn gray2fill(g: f32) -> Rgba8 {
    rgb2fill(g, g, g)
}
fn cymk2fill(c: f32, y: f32, m: f32, k: f32) -> Rgba8 {
    rgb2fill(
        (1.0 - c) * (1.0 - k),
        (1.0 - m) * (1.0 - k),
        (1.0 - y) * (1.0 - k)
    )
}

#[derive(Debug)]
enum TextEncoding {
    CID,
    Cmap(HashMap<u16, GlyphId>)
}

struct FontEntry<S: Surface> {
    font: Box<dyn Font<S::Outline>>,
    encoding: TextEncoding,
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
struct GraphicsState {
    transform: Transform2F,
    stroke_width: f32,
    fill_color: Rgba8,
    stroke_color: Rgba8
}
impl GraphicsState {
    fn get_text_style(&self, mode: TextMode) -> PathStyle {
        match mode {
            TextMode::Fill => self.fill_style(),
            TextMode::Stroke => self.stroke_style(),
            TextMode::FillThenStroke => self.fill_and_stroke_style(),
            _ => PathStyle {
                fill: None,
                stroke: None,
                fill_rule: FillRule::NonZero,
            }
        }
    }
    fn fill_style(&self) -> PathStyle {
        PathStyle {
            fill: Some(self.fill_color),
            stroke: None,
            fill_rule: FillRule::NonZero,
        }
    }
    fn stroke_style(&self) -> PathStyle {
        PathStyle {
            fill: None,
            stroke: Some((self.stroke_color, self.stroke_width)),
            fill_rule: FillRule::NonZero,
        }
    }
    fn fill_and_stroke_style(&self) -> PathStyle {
        PathStyle {
            fill: Some(self.fill_color),
            stroke: Some((self.stroke_color, self.stroke_width)),
            fill_rule: FillRule::NonZero,
        }
    }
}

struct TextState<'a, S: Surface> {
    root_transform: Transform2F,
    text_matrix: Transform2F, // tracks current glyph
    line_matrix: Transform2F, // tracks current line
    char_space: f32, // Character spacing
    word_space: f32, // Word spacing
    horiz_scale: f32, // Horizontal scaling
    leading: f32, // Leading
    font_entry: Option<&'a FontEntry<S>>, // Text font
    font_size: f32, // Text font size
    mode: TextMode, // Text rendering mode
    rise: f32, // Text rise
    knockout: f32, //Text knockout
}
impl<'a, S: Surface> Clone for TextState<'a, S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a, S: Surface> Copy for TextState<'a, S> {}
impl<'a, S: Surface> TextState<'a, S> {
    fn new(root_transform: Transform2F) -> TextState<'a, S> {
        TextState {
            root_transform,
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
    fn reset_matrix(&mut self, root_tansformation: Transform2F) {
        self.root_transform = root_tansformation;
        self.set_matrix(Transform2F::default());
    }
    fn translate(&mut self, v: Vector2F) {
        let m = self.line_matrix * Transform2F::from_translation(v);
        self.set_matrix(m);
    }
    
    // move to the next line
    fn next_line(&mut self) {
        self.translate(Vector2F::new(0., -self.leading * self.font_size));
    }
    // set text and line matrix
    fn set_matrix(&mut self, m: Transform2F) {
        self.text_matrix = m;
        self.line_matrix = m;
    }
    fn add_glyphs(&mut self, surface: &mut S, glyphs: impl Iterator<Item=(u16, Option<GlyphId>)>, style: &S::Style) {
        let e = self.font_entry.as_ref().expect("no font");

        let tr = Transform2F::row_major(
            self.horiz_scale * self.font_size, 0., 0.,
            self.font_size, 0., self.rise) * e.font.font_matrix();
        
        for (cid, gid) in glyphs {
            debug!("cid {} -> gid {:?}", cid, gid);
            let gid = match gid {
                Some(gid) => gid,
                None => {
                    continue;
                }
            };
            if let Some(glyph) = e.font.glyph(gid) {
                let transform = self.root_transform * self.text_matrix * tr;
                let path = glyph.path.transform(transform);
                surface.draw_path(path, &style);
                
                let dx = match cid {
                    0x20 => self.word_space,
                    _ => self.char_space
                };
                let advance = dx * self.horiz_scale * self.font_size + tr.m11() * glyph.metrics.advance.x();
                self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
            } else {
                info!("no glyph for gid {:?}", gid);
            }
        }
    }
    fn draw_text(&mut self, surface: &mut S, data: &[u8], style: &S::Style) {
        debug!("text: {:?}", String::from_utf8_lossy(data));
        if let Some(e) = self.font_entry {
            let get_glyph = |cid: u16| {
                let gid = match e.encoding {
                    TextEncoding::CID => Some(GlyphId(cid as u32)),
                    TextEncoding::Cmap(ref cmap) => cmap.get(&cid).cloned()
                };
                (cid, gid)
            };
            if e.is_cid {
                self.add_glyphs(
                    surface,
                    data.chunks_exact(2).map(|s| get_glyph(u16::from_be_bytes(s.try_into().unwrap()))),
                    style
                );
            } else {
                self.add_glyphs(surface, data.iter().map(|&b| get_glyph(b as u16)), style);
            }
        } else {
            warn!("no font set");
        }
    }
    fn advance(&mut self, delta: f32) {
        //debug!("advance by {}", delta);
        let advance = delta * self.font_size * self.horiz_scale;
        self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
    }
}

pub struct Cache<S: Surface> {
    // shared mapping of fontname -> font
    fonts: HashMap<String, FontEntry<S>>
}
impl<S: Surface> FontEntry<S> {
    fn build(font: Box<dyn Font<S::Outline>>, pdf_font: &PdfFont) -> FontEntry<S> {
        let mut is_cid = pdf_font.is_cid();
        let encoding = pdf_font.encoding().clone();
        
        let encoding = if let Some(map) = pdf_font.cid_to_gid_map() {
            is_cid = true;
            let cmap = map.iter().enumerate().map(|(cid, &gid)| (cid as u16, GlyphId(gid as u32))).collect();
            TextEncoding::Cmap(cmap)
        } else if encoding.base == BaseEncoding::IdentityH {
            is_cid = true;
            TextEncoding::CID
        } else {
            let mut cmap = HashMap::new();
            let source_encoding = match encoding.base {
                BaseEncoding::StandardEncoding => Some(Encoding::AdobeStandard),
                BaseEncoding::SymbolEncoding => Some(Encoding::AdobeSymbol),
                BaseEncoding::WinAnsiEncoding => Some(Encoding::WinAnsiEncoding),
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
                    warn!("can't translate from text encoding {:?} to font encoding {:?}", encoding.base, font_encoding);
                    
                    // assuming same encoding
                    for cp in 0 .. 256 {
                        if let Some(gid) = font.gid_for_codepoint(cp) {
                            cmap.insert(cp as u16, gid);
                        }
                    }
                }
            }
            for (&cp, name) in encoding.differences.iter() {
                debug!("{} -> {}", cp, name);
                match font.gid_for_name(&name) {
                    Some(gid) => {
                        cmap.insert(cp as u16, gid);
                    }
                    None => info!("no glyph for name {}", name)
                }
            }
            debug!("cmap: {:?}", cmap);
            TextEncoding::Cmap(cmap)
        };
        
        FontEntry {
            font: font,
            encoding,
            is_cid,
        }
    }
}


impl<S> Cache<S> where S: Surface + 'static, S::Outline: Sync + Send {
    pub fn new() -> Cache<S> {
        Cache {
            fonts: HashMap::new()
        }
    }
    fn load_font(&mut self, pdf_font: &PdfFont) {
        if self.fonts.get(&pdf_font.name).is_some() {
            return;
        }
        
        debug!("loading {:?}", pdf_font);
        
        let data: Cow<[u8]> = match (pdf_font.standard_font(), pdf_font.embedded_data()) {
            (_, Some(Ok(data))) => {
                if let Some(path) = std::env::var_os("PDF_FONTS") {
                    let file = PathBuf::from(path).join(&pdf_font.name);
                    fs::write(file, data).expect("can't write font");
                }
                data.into()
            }
            (Some(data), _) => data.into(),
            (None, Some(Err(e))) => panic!("can't decode font data: {:?}", e),
            (None, None) => {
                info!("Font: {:?}", pdf_font);
                warn!("No font data for {}. Glyphs will be missing.", pdf_font.name);
                return;
            }
        };
        let entry = FontEntry::build(font::parse(&data), pdf_font);
        debug!("is_cid={}", entry.is_cid);
            
        self.fonts.insert(pdf_font.name.clone(), entry);
    }
    fn get_font(&self, font_name: &str) -> Option<&FontEntry<S>> {
        self.fonts.get(font_name)
    }
    
    pub fn render_page<B: Backend>(&mut self, file: &PdfFile<B>, page: &Page) -> Result<S> {
        let Rect { left, right, top, bottom } = page.media_box(file).expect("no media box");
        let rect = RectF::from_points(Vector2F::new(left, bottom), Vector2F::new(right, top));
        
        let scale = Vector2F::splat(0.5);
        let mut surface = S::new(rect.size() * scale);
        
        let mut path_builder = PathBuilder::<S::Outline>::new();

        // draw the page
        let style = surface.build_style(PathStyle {
            fill: Some((255,255,255,255)),
            stroke: Some(((0,0,0,255), 0.25)),
            fill_rule: FillRule::NonZero,
        });
        path_builder.rect(RectF::new(Vector2F::default(), rect.size() * scale));
        surface.draw_path(path_builder.take(), &style);

        let root_tansformation = Transform2F::from_scale(scale) * Transform2F::row_major(1.0, 0.0, 0.0, -1.0, -left, top);
        
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
        

        let mut last = Vector2F::default();
        let mut text_state = TextState::new(root_tansformation);
        let mut stack = vec![];

        let mut graphics_state = GraphicsState {
            transform: root_tansformation,
            stroke_width: 0.0,
            fill_color: (0, 0, 0, 255),
            stroke_color: (0, 0, 0, 255),
        };
        
        let mut iter = try_opt!(page.contents.as_ref()).operations.iter();
        while let Some(op) = iter.next() {
            debug!("{}", op);
            let ref ops = op.operands;
            match op.operator.as_str() {
                "m" => { // move x y
                    ops_p!(ops, p => {
                        path_builder.move_to(p);
                        last = p;
                    })
                }
                "l" => { // line x y
                    ops_p!(ops, p => {
                        path_builder.line_to(p);
                        last = p;
                    })
                }
                "c" => { // cubic bezier c1.x c1.y c2.x c2.y p.x p.y
                    ops_p!(ops, c1, c2, p => {
                        path_builder.cubic_curve_to(c1, c2, p);
                        last = p;
                    })
                }
                "v" => { // cubic bezier c2.x c2.y p.x p.y
                    ops_p!(ops, c2, p => {
                        path_builder.cubic_curve_to(last, c2, p);
                        last = p;
                    })
                }
                "y" => { // cubic c1.x c1.y p.x p.y
                    ops_p!(ops, c1, p => {
                        path_builder.cubic_curve_to(c1, p, p);
                        last = p;
                    })
                }
                "h" => { // close
                    path_builder.close();
                }
                "re" => { // rect x y width height
                    ops_p!(ops, origin, size => {
                        let r = RectF::new(origin, size);
                        path_builder.rect(r);
                    })
                }
                "S" => { // stroke
                    let style = surface.build_style(graphics_state.stroke_style());
                    let path = path_builder.take().transform(graphics_state.transform);
                    surface.draw_path(path, &style);
                }
                "s" => { // close and stroke
                    path_builder.close();
                    let style = surface.build_style(graphics_state.stroke_style());
                    let path = path_builder.take().transform(graphics_state.transform);
                    surface.draw_path(path, &style);
                }
                "f" | "F" | "f*" => { // close and fill 
                    // TODO: implement windings
                    path_builder.close();
                    let path = path_builder.take().transform(graphics_state.transform);
                    let style = surface.build_style(graphics_state.fill_style());
                    surface.draw_path(path, &style);
                }
                "B" | "B*" => { // fill and stroke
                    path_builder.close();
                    let path = path_builder.take().transform(graphics_state.transform);
                    let style = surface.build_style(graphics_state.fill_and_stroke_style());
                    surface.draw_path(path, &style);
                }
                "b" | "b*" => { // stroke and fill
                    path_builder.close();
                    let path = path_builder.take().transform(graphics_state.transform);
                    let style = surface.build_style(graphics_state.fill_and_stroke_style());
                    surface.draw_path(path, &style);
                }
                "n" => { // clear path
                    path_builder.clear();
                }
                "W" | "W*" => { // clipping path
                }
                "q" => { // save state
                    stack.push((graphics_state, text_state));
                }
                "Q" => { // restore
                    let (g, t) = stack.pop().expect("graphcs stack is empty");
                    graphics_state = g;
                    text_state = t;
                }
                "cm" => { // modify transformation matrix 
                    ops!(ops, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 => {
                        graphics_state.transform = graphics_state.transform * Transform2F::row_major(a, b, c, d, e, f);
                    })
                }
                "w" => { // line width
                    ops!(ops, width: f32 => {
                        graphics_state.stroke_width = width;
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
                "gs" => ops!(ops, gs: &str => { // set from graphic state dictionary
                    let gs = try_opt!(resources.graphics_states.get(gs));
                    
                    if let Some(lw) = gs.line_width {
                        graphics_state.stroke_width = lw;
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
                "W" | "W*" => { // clipping path
                
                }
                "SC" | "RG" => { // stroke color
                    ops!(ops, r: f32, g: f32, b: f32 => {
                        graphics_state.stroke_color = rgb2fill(r, g, b);
                    });
                }
                "sc" | "rg" => { // fill color
                    ops!(ops, r: f32, g: f32, b: f32 => {
                        graphics_state.fill_color = rgb2fill(r, g, b);
                    });
                }
                "G" => { // stroke gray
                    ops!(ops, gray: f32 => {
                        graphics_state.stroke_color = gray2fill(gray);
                    })
                }
                "g" => { // fill gray
                    ops!(ops, gray: f32 => {
                        graphics_state.fill_color = gray2fill(gray);
                    })
                }
                "k" => { // fill color
                    ops!(ops, c: f32, y: f32, m: f32, k: f32 => {
                        graphics_state.fill_color = cymk2fill(c, y, m, k);
                    });
                }
                "cs" => { // color space
                }
                "BT" => {
                    text_state.reset_matrix(graphics_state.transform);
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
                "Tf" => ops!(ops, font_name: &str, size: f32 => {
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
                    text_state.set_matrix(Transform2F::row_major(a, b, c, d, e, f));
                }),
                
                // Move to the start of the next line
                "T*" => {
                    text_state.next_line();
                },
                
                // draw text
                "Tj" => ops!(ops, text: &[u8] => {
                    let style = surface.build_style(graphics_state.get_text_style(text_state.mode));
                    text_state.draw_text(&mut surface, text, &style);
                }),
                
                // move to the next line and draw text
                "'" => ops!(ops, text: &[u8] => {
                    let style = surface.build_style(graphics_state.get_text_style(text_state.mode));
                    text_state.next_line();
                    text_state.draw_text(&mut surface, text, &style);
                }),
                
                // set word and charactr spacing, move to the next line and draw text
                "\"" => ops!(ops, word_space: f32, char_space: f32, text: &[u8] => {
                    let style = surface.build_style(graphics_state.get_text_style(text_state.mode));
                    text_state.word_space = word_space;
                    text_state.char_space = char_space;
                    text_state.next_line();
                    text_state.draw_text(&mut surface, text, &style);
                }),
                "TJ" => ops!(ops, array: &[Primitive] => {
                    let style = surface.build_style(graphics_state.get_text_style(text_state.mode));
                    for arg in array {
                        match arg {
                            Primitive::String(ref data) => {
                                text_state.draw_text(&mut surface, data.as_bytes(), &style);
                            },
                            p => {
                                let offset = p.as_number().expect("wrong argument to TJ");
                                text_state.advance(-0.001 * offset); // because why not PDF…
                            }
                        }
                    }
                }),
                _ => {}
            }
        }
        
        Ok(surface)
    }
}
