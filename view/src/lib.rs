#[macro_use] extern crate log;
#[macro_use] extern crate pdf;
extern crate env_logger;

use std::io::Write;
use std::mem;
use std::convert::TryInto;
use std::path::Path;
use std::collections::HashMap;
use std::fs;
use std::borrow::Cow;

use pdf::file::File as PdfFile;
use pdf::object::*;
use pdf::primitive::Primitive;
use pdf::backend::Backend;
use pdf::font::{Font as PdfFont, FontType};
use pdf::error::{PdfError, Result};
use pdf::encoding::{Encoding};

use pathfinder_content::color::ColorU;
use pathfinder_geometry::{
    vector::Vector2F, rect::RectF, transform2d::Transform2F
};
use pathfinder_canvas::{CanvasRenderingContext2D, CanvasFontContext, Path2D, FillStyle};
use pathfinder_renderer::scene::Scene;
use font::{Font, BorrowedFont, CffFont, TrueTypeFont, Type1Font, Glyphs, parse_opentype};

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
        || -> Result<()> {
            let mut iter = $ops.iter();
            $(
                let $var: $typ = iter.next().ok_or(PdfError::EOF)?.try_into()?;
            )*
            $block;
            Ok(())
        }();
    })
}

type P = Vector2F;
fn rgb2fill(r: f32, g: f32, b: f32) -> FillStyle {
    let c = |v: f32| (v * 255.) as u8;
    FillStyle::Color(ColorU { r: c(r), g: c(g), b: c(b), a: 255 })
}
fn gray2fill(g: f32) -> FillStyle {
    rgb2fill(g, g, g)
}
fn cymk2fill(c: f32, y: f32, m: f32, k: f32) -> FillStyle {
    rgb2fill(
        (1.0 - c) * (1.0 - k),
        (1.0 - m) * (1.0 - k),
        (1.0 - y) * (1.0 - k)
    )
}

struct FontEntry {
    glyphs: Glyphs,
    font_matrix: Transform2F,
    cmap: Option<HashMap<u16, u32>>, // codepoint -> glyph id
    encoding: Encoding,
    is_cid: bool
}
enum TextMode {
    Fill,
    Stroke,
    FillThenStroke,
    Invisible,
    FillAndClip,
    StrokeAndClip
}

struct TextState<'a> {
    text_matrix: Transform2F, // tracks current glyph
    line_matrix: Transform2F, // tracks current line
    char_space: f32, // Character spacing
    word_space: f32, // Word spacing
    horiz_scale: f32, // Horizontal scaling
    leading: f32, // Leading
    font: Option<&'a FontEntry>, // Text font
    font_size: f32, // Text font size
    mode: TextMode, // Text rendering mode
    rise: f32, // Text rise
    knockout: f32 //Text knockout
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
            font: None,
            font_size: 0.,
            mode: TextMode::Fill,
            rise: 0.,
            knockout: 0.
        }
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
    fn add_glyphs(&mut self, canvas: &mut CanvasRenderingContext2D, glyphs: impl Iterator<Item=(u32, bool)>) {
        let font = self.font.as_ref().unwrap();
        let base = canvas.current_transform();

        let tr = Transform2F::row_major(
            self.horiz_scale * self.font_size, 0., 0.,
            self.font_size, 0., self.rise) * font.font_matrix;
        
        for (gid, is_space) in glyphs {
            let glyph = font.glyphs.get(gid as u32).unwrap();
            
            let transform = base * self.text_matrix * tr;
            
            canvas.set_current_transform(&transform);
            canvas.fill_path(glyph.path.clone());
            
            let dx = match is_space {
                true => self.word_space,
                false => self.char_space
            };
            debug!("glyph {} has width: {}", gid, glyph.width);
            let advance = dx + tr.m11() * glyph.width;
            self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
        }
        
        canvas.set_current_transform(&base);
    }
    fn add_text_cid(&mut self, canvas: &mut CanvasRenderingContext2D, data: &[u8]) {
        self.add_glyphs(canvas, data.chunks_exact(2).map(|s| {
            let sid = u16::from_be_bytes(s.try_into().unwrap());
            (sid as u32, sid == 0x20)
        }));
    }
    fn draw_text(&mut self, canvas: &mut CanvasRenderingContext2D, data: &[u8]) {
        if let Some(font) = self.font {
            if font.is_cid {
                return self.add_text_cid(canvas, data);
            }
            
            let cmap = font.cmap.as_ref().expect("no cmap");
            self.add_glyphs(canvas, data.iter().filter_map(|&b| {
                match cmap.get(&(b as u16)) {
                    Some(&gid) => {
                        debug!("byte {} -> gid {}", b, gid);
                        Some((gid, b == 0x20))
                    },
                    None => {
                        debug!("byte {} has no gid", b);
                        None
                    }
                }
            }));
        }
    }
    fn advance(&mut self, delta: f32) {
        let advance = delta * self.font_size * self.horiz_scale;
        self.text_matrix = self.text_matrix * Transform2F::from_translation(Vector2F::new(advance, 0.));
    }
}

pub struct Cache {
    // shared mapping of fontname -> font
    fonts: HashMap<String, FontEntry>
}
impl FontEntry {
    fn build<'a>(font: Box<dyn BorrowedFont<'a> + 'a>, encoding: Encoding) -> FontEntry {
        let decode = false;
    
        // build cmap
        let mut cmap = HashMap::new();
        let decode_one = |b: u8| -> Option<u32> {
            let cp = match decode {
                true => encoding.base.decode_byte(b)? as u32,
                false => b as u32
            };
            font.gid_for_codepoint(cp)
        };
            
        for b in 0 ..= 255 {
            if let Some(gid) = decode_one(b) {
                cmap.insert(b as u16, gid);
            }
        }
        for (&cp, name) in encoding.differences.iter() {
            debug!("{} -> {}", cp, name);
            let gid = font.gid_for_name(&name).expect("no such glyph");
            cmap.insert(cp as u16, gid);
        }
        
        FontEntry {
            glyphs: font.glyphs(),
            cmap: Some(cmap),
            encoding: encoding.clone(),
            is_cid: false,
            font_matrix: font.font_matrix()
        }
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
        let encoding = pdf_font.encoding().clone();
        
        let data: Cow<[u8]> = match (pdf_font.standard_font(), pdf_font.embedded_data()) {
            (_, Some(Ok(data))) => data.into(),
            (Some(filename), _) => {
                let font_path = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
                    .join("fonts")
                    .join(filename);
                fs::read(font_path).unwrap().into()
            }
            (None, Some(Err(e))) => panic!("can't decode font data: {:?}", e),
            (None, None) => {
                info!("Font: {:?}", pdf_font);
                warn!("No font data for {}. Glyphs will be missing.", pdf_font.name);
                return;
            }
        };
        let mut entry = FontEntry::build(font::parse(&data), encoding);
                
        match pdf_font.subtype {
            FontType::CIDFontType0 | FontType::CIDFontType2 => entry.is_cid = true,
            _ => {}
        }
        debug!("is_cid={}", entry.is_cid);
            
        self.fonts.insert(pdf_font.name.clone(), entry);
    }
    fn get_font(&self, font_name: &str) -> Option<&FontEntry> {
        self.fonts.get(font_name)
    }
    
    pub fn render_page<B: Backend>(&mut self, file: &PdfFile<B>, page: &Page) -> Result<Scene> {
        let Rect { left, right, top, bottom } = page.media_box(file).expect("no media box");
        
        let resources = page.resources(file)?;
        
        let rect = RectF::from_points(Vector2F::new(left, bottom), Vector2F::new(right, top));
        
        let mut canvas = CanvasRenderingContext2D::new(CanvasFontContext::from_system_source(), rect.size());
        canvas.stroke_rect(RectF::new(Vector2F::default(), rect.size()));
        let root_tansformation = Transform2F::row_major(1.0, 0.0, 0.0, -1.0, -left, top);
        canvas.set_current_transform(&root_tansformation);
        debug!("transform: {:?}", canvas.current_transform());
        
        // make sure all fonts are in the cache, so we can reference them
        for font in resources.fonts.values() {
            self.load_font(font);
        }
        for gs in resources.graphics_states.values() {
            if let Some((ref font, _)) = gs.font {
                self.load_font(font);
            }
        }
        
        let mut path = Path2D::new();
        let mut last = Vector2F::default();
        let mut state = TextState::new();
        
        let mut iter = try_opt!(page.contents.as_ref()).operations.iter();
        while let Some(op) = iter.next() {
            debug!("{}", op);
            let ref ops = op.operands;
            match op.operator.as_str() {
                "m" => { // move x y
                    ops_p!(ops, p => {
                        path.move_to(p);
                        last = p;
                    })
                }
                "l" => { // line x y
                    ops_p!(ops, p => {
                        path.line_to(p);
                        last = p;
                    })
                }
                "c" => { // cubic bezier c1.x c1.y c2.x c2.y p.x p.y
                    ops_p!(ops, c1, c2, p => {
                        path.bezier_curve_to(c1, c2, p);
                        last = p;
                    })
                }
                "v" => { // cubic bezier c2.x c2.y p.x p.y
                    ops_p!(ops, c2, p => {
                        path.bezier_curve_to(last, c2, p);
                        last = p;
                    })
                }
                "y" => { // cubic c1.x c1.y p.x p.y
                    ops_p!(ops, c1, p => {
                        path.bezier_curve_to(c1, p, p);
                        last = p;
                    })
                }
                "h" => { // close
                    path.close_path();
                }
                "re" => { // rect x y width height
                    ops_p!(ops, origin, size => {
                        let r = RectF::new(origin, size);
                        path.rect(r);
                    })
                }
                "S" => { // stroke
                    canvas.stroke_path(mem::replace(&mut path, Path2D::new()));
                }
                "s" => { // close and stroke
                    path.close_path();
                    canvas.stroke_path(mem::replace(&mut path, Path2D::new()));
                }
                "f" | "F" | "f*" => { // close and fill 
                    // TODO: implement windings
                    path.close_path();
                    canvas.fill_path(mem::replace(&mut path, Path2D::new()));
                }
                "B" | "B*" => { // fill and stroke
                    path.close_path();
                    let path2 = mem::replace(&mut path, Path2D::new());
                    canvas.fill_path(path2.clone());
                    canvas.stroke_path(path2);
                }
                "b" | "b*" => { // stroke and fill
                    path.close_path();
                    let path2 = mem::replace(&mut path, Path2D::new());
                    canvas.stroke_path(path2.clone());
                    canvas.fill_path(path2);
                }
                "n" => { // clear path
                    path = Path2D::new();
                }
                "q" => { // save state
                    canvas.save();
                }
                "Q" => { // restore
                    canvas.restore();
                }
                "cm" => { // modify transformation matrix 
                    ops!(ops, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 => {
                        let tr = canvas.current_transform() * Transform2F::row_major(a, b, c, d, e, f);
                        canvas.set_current_transform(&tr);
                    })
                }
                "w" => { // line width
                    ops!(ops, width: f32 => {
                        canvas.set_line_width(width);
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
                        canvas.set_line_width(lw);
                    }
                    if let Some((ref font, size)) = gs.font {
                        if let Some(e) = self.get_font(&font.name) {
                            state.font = Some(e);
                            state.font_size = size;
                            debug!("new font: {} at size {}", font.name, size);
                        } else {
                            state.font = None;
                        }
                    }
                }),
                "W" | "W*" => { // clipping path
                
                }
                "SC" | "RG" => { // stroke color
                    ops!(ops, r: f32, g: f32, b: f32 => {
                        canvas.set_stroke_style(rgb2fill(r, g, b));
                    });
                }
                "sc" | "rg" => { // fill color
                    ops!(ops, r: f32, g: f32, b: f32 => {
                        canvas.set_fill_style(rgb2fill(r, g, b));
                    });
                }
                "G" => { // stroke gray
                    ops!(ops, gray: f32 => {
                        canvas.set_stroke_style(gray2fill(gray));
                    })
                }
                "g" => { // stroke gray
                    ops!(ops, gray: f32 => {
                        canvas.set_fill_style(gray2fill(gray));
                    })
                }
                "k" => { // fill color
                    ops!(ops, c: f32, y: f32, m: f32, k: f32 => {
                        canvas.set_fill_style(cymk2fill(c, y, m, k));
                    });
                }
                "cs" => { // color space
                }
                "BT" => {
                    state = TextState::new();
                }
                "ET" => {
                    state.font = None;
                }
                // state modifiers
                
                // character spacing
                "Tc" => ops!(ops, char_space: f32 => {
                        state.char_space = char_space;
                }),
                
                // word spacing
                "Tw" => ops!(ops, word_space: f32 => {
                        state.word_space = word_space;
                }),
                
                // Horizontal scaling (in percent)
                "Tz" => ops!(ops, scale: f32 => {
                        state.horiz_scale = 0.01 * scale;
                }),
                
                // leading
                "TL" => ops!(ops, leading: f32 => {
                        state.leading = leading;
                }),
                
                // text font
                "Tf" => ops!(ops, font_name: &str, size: f32 => {
                    let font = try_opt!(resources.fonts.get(font_name));
                    if let Some(e) = self.get_font(&font.name) {
                        state.font = Some(e);
                        debug!("new font: {}", font.name);
                        state.font_size = size;
                    } else {
                        state.font = None;
                    }
                }),
                
                // render mode
                "Tr" => ops!(ops, mode: i32 => {
                    use TextMode::*;
                    state.mode = match mode {
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
                    state.rise = rise;
                }),
                
                // positioning operators
                // Move to the start of the next line
                "Td" => ops_p!(ops, t => {
                    state.translate(t);
                }),
                
                "TD" => ops_p!(ops, t => {
                    state.leading = -t.y();
                    state.translate(t);
                }),
                
                // Set the text matrix and the text line matrix
                "Tm" => ops!(ops, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 => {
                    state.set_matrix(Transform2F::row_major(a, b, c, d, e, f));
                }),
                
                // Move to the start of the next line
                "T*" => {
                    state.next_line();
                },
                
                // draw text
                "Tj" => ops!(ops, text: &[u8] => {
                    state.draw_text(&mut canvas, text);
                }),
                
                // move to the next line and draw text
                "'" => ops!(ops, text: &[u8] => {
                    state.next_line();
                    state.draw_text(&mut canvas, text);
                }),
                
                // set word and charactr spacing, move to the next line and draw text
                "\"" => ops!(ops, word_space: f32, char_space: f32, text: &[u8] => {
                    state.word_space = word_space;
                    state.char_space = char_space;
                    state.next_line();
                    state.draw_text(&mut canvas, text);
                }),
                "TJ" => ops!(ops, array: &[Primitive] => {
                    if let Some(font) = state.font {
                        let mut text: Vec<u8> = Vec::new();
                        for arg in array {
                            match arg {
                                Primitive::String(ref data) => {
                                    state.draw_text(&mut canvas, data.as_bytes());
                                    text.extend(data.as_bytes());
                                },
                                p => {
                                    let offset = p.as_number().expect("wrong argument to TJ");
                                    state.advance(-0.001 * offset); // because why not PDF…
                                }
                            }
                        }
                        debug!("Text: {}", font.encoding.base.decode_bytes(&text));
                    }
                }),
                _ => {}
            }
        }
        
        Ok(canvas.into_scene())
    }
}
