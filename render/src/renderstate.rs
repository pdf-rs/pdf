use std::convert::TryInto;
use std::sync::Arc;

use pdf::file::File as PdfFile;
use pdf::object::*;
use pdf::primitive::Primitive;
use pdf::backend::Backend;
use pdf::content::Operation;
use pdf::error::{PdfError, Result};

use pathfinder_geometry::{
    vector::{Vector2F, Vector2I},
    rect::RectF, transform2d::Transform2F,
};
use pathfinder_content::{
    fill::FillRule,
    stroke::{LineCap, LineJoin, StrokeStyle},
    outline::{Outline, Contour},
    pattern::{Pattern, Image},
};
use pathfinder_color::ColorU;
use pathfinder_renderer::{
    scene::{DrawPath, ClipPath, Scene},
    paint::{Paint},
};

use super::{
    graphicsstate::{GraphicsState, DrawMode},
    textstate::{TextState, TextMode},
    cache::{Cache},
    BBox,
};

pub struct RenderState<'a, B: Backend> {
    graphics_state: GraphicsState<'a>,
    text_state: TextState<'a>,
    stack: Vec<(GraphicsState<'a>, TextState<'a>)>,
    current_outline: Outline,
    current_contour: Contour,
    scene: &'a mut Scene,
    file: &'a PdfFile<B>,
    resources: &'a Resources,
    cache: &'a Cache,
}

/*
use phf::phf_match;
macro_rules! op_match {
    (($self:ident, $arg:ident), { $($name:tt => $fun:ident,)* }) => {
        phf_match!{
            $($name => $self.$fun($arg), )*
            _ => Ok(())
        }
    };
}
*/
macro_rules! op_match {
    (($self:ident, $arg:ident), { $($name:tt => $fun:ident,)* }) => {
        |key| match key {
            $($name => $self.$fun($arg), )*
            _ => Ok(())
        }
    };
}


impl<'a, B: Backend> RenderState<'a, B> {
    pub fn new(cache: &'a Cache, scene: &'a mut Scene, file: &'a PdfFile<B>, resources: &'a Resources, root_transformation: Transform2F) -> Self {
        let black = scene.push_paint(&Paint::from_color(ColorU::black()));

        let graphics_state = GraphicsState {
            transform: root_transformation,
            fill_paint: black,
            stroke_paint: black,
            clip_path: None,
            fill_color_space: &ColorSpace::DeviceRGB,
            stroke_color_space: &ColorSpace::DeviceRGB,
            stroke_style: StrokeStyle {
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter(1.0),
                line_width: 1.0,
            }
        };
        let text_state = TextState::new();
        let stack = vec![];
        let current_outline = Outline::new();
        let current_contour = Contour::new();

        RenderState {
            graphics_state,
            text_state,
            stack,
            current_outline,
            current_contour,
            scene,
            cache,
            resources,
            file,
        }
    }
    pub fn draw_op(&mut self, op: &Operation) -> Result<()> {
        debug!("{}", op);
        let s = op.operator.as_str();
        let ops = &op.operands;

        let mut f = op_match!((self, ops), {
            "m" => op_m,
            "l" => op_l,
            "c" => op_c,
            "v" => op_v,
            "y" => op_y,
            "h" => op_h,
            "re" => op_re,
            "S" => op_S,
            "s" => op_s,
            "f" => op_f,
            "f*" => op_f_star,
            "B" => op_B,
            "B*" => op_B_star,
            "b" => op_b,
            "b*" => op_b_star,
            "n" => op_n,
            "W" => op_W,
            "W*" => op_W_star,
            "q" => op_q,
            "Q" => op_Q,
            "cm" => op_cm,
            "w" => op_w,
            "J" => op_nop, // line cap
            "j" => op_nop, // line join
            "M" => op_nop, // miter limit
            "d" => op_nop, // line dash [ array phase ]
            "gs" => op_gs,
            "SC" => op_stroke_color,
            "SCN" => op_stroke_color,
            "RG" => op_stroke_color,
            "sc" => op_fill_color,
            "scn" => op_fill_color,
            "rg" => op_fill_color,
            "G" => op_G,
            "g" => op_g,
            "K" => op_K,
            "k" => op_k,
            "cs" => op_cs,
            "CS" => op_CS,
            "BT" => op_BT,
            "ET" => op_ET,
            "Tc" => op_Tc,
            "Tw" => op_Tw,
            "Tz" => op_Tz,
            "TL" => op_TL,
            "Tf" => op_Tf,
            "Tr" => op_Tr,
            "Ts" => op_Ts,
            "Td" => op_Td,
            "TD" => op_TD,
            "Tm" => op_Tm,
            "T*" => op_T_star,
            "Tj" => op_Tj,
            "'" => op_tick,
            "\"" => op_doubletick,
            "TJ" => op_TJ,
            "Do" => op_Do,
        });
        ctx!(f(s), op)
    }

    fn trace_bbox(&mut self, bb: BBox) {

    }
    fn trace_rect(&mut self, rect: RectF) {

    }
}

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


type OpArgs = [Primitive];

fn convert_color(cs: &ColorSpace, ops: &OpArgs) -> Result<Paint> {
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

#[allow(non_snake_case, unused_variables)]
impl<'a, B: Backend> RenderState<'a, B> {
    fn flush(&mut self) {
        if !self.current_contour.is_empty() {
            self.current_outline.push_contour(self.current_contour.clone());
            self.current_contour.clear();
        }
    }
    fn op_m(&mut self, ops: &OpArgs) -> Result<()> {
        // move x y
        ops_p!(ops, p => {
            self.flush();
            self.current_contour.push_endpoint(p);
        });
        Ok(())
    }
    fn op_l(&mut self, ops: &OpArgs) -> Result<()> {
        // line x y
        ops_p!(ops, p => {
            self.current_contour.push_endpoint(p);
        });
        Ok(())
    }
    fn op_c(&mut self, ops: &OpArgs) -> Result<()> {
        // cubic bezier c1.x c1.y c2.x c2.y p.x p.y
        ops_p!(ops, c1, c2, p => {
            self.current_contour.push_cubic(c1, c2, p);
        });
        Ok(())
    }
    fn op_v(&mut self, ops: &OpArgs) -> Result<()> {
        // cubic bezier c2.x c2.y p.x p.y
        ops_p!(ops, c2, p => {
            let c1 = self.current_contour.last_position().unwrap_or_default();
            self.current_contour.push_cubic(c1, c2, p);
        });
        Ok(())
    }
    fn op_y(&mut self, ops: &OpArgs) -> Result<()> {
        // cubic c1.x c1.y p.x p.y
        ops_p!(ops, c1, p => {
            self.current_contour.push_cubic(c1, p, p);
        });
        Ok(())
    }
    fn op_h(&mut self, ops: &OpArgs) -> Result<()> {
        // close
        self.current_contour.close();
        Ok(())
    }
    fn op_re(&mut self, ops: &OpArgs) -> Result<()> {
        // rect x y width height
        ops_p!(ops, origin, size => {
            self.flush();
            self.current_outline.push_contour(Contour::from_rect(RectF::new(origin, size)));
        });
        Ok(())
    }
    fn op_S(&mut self, ops: &OpArgs) -> Result<()> {
        // stroke
        self.flush();
        self.graphics_state.draw(self.scene, &self.current_outline, DrawMode::Stroke, FillRule::Winding);
        self.current_outline.clear();
        Ok(())
    }
    fn op_s(&mut self, ops: &OpArgs) -> Result<()> {
        // close and stroke
        self.current_contour.close();
        self.flush();
        self.graphics_state.draw(self.scene, &self.current_outline, DrawMode::Stroke, FillRule::Winding);
        self.current_outline.clear();
        Ok(())
    }
    fn close_and_fill(&mut self, fill_rule: FillRule) {
        self.current_contour.close();
        self.flush();
        self.graphics_state.draw(self.scene, &self.current_outline, DrawMode::Fill, fill_rule);
        self.current_outline.clear();
    }
    fn op_f(&mut self, ops: &OpArgs) -> Result<()> {
        // close and fill (winding)
        self.close_and_fill(FillRule::Winding);
        Ok(())
    }
    fn op_f_star(&mut self, ops: &OpArgs) -> Result<()> {
        // close and fill (even-odd)
        self.close_and_fill(FillRule::EvenOdd);
        Ok(())
    }
    fn fill_stroke(&mut self, fill_rule: FillRule) {
        self.flush();
        self.graphics_state.draw(self.scene, &self.current_outline, DrawMode::FillStroke, fill_rule);
        self.current_outline.clear();
    }
    fn op_B(&mut self, ops: &OpArgs) -> Result<()> {
        // fill and stroke (winding)
        self.fill_stroke(FillRule::Winding);
        Ok(())
    }
    fn op_B_star(&mut self, ops: &OpArgs) -> Result<()> {
        // fill and stroke (even-odd)
        self.fill_stroke(FillRule::EvenOdd);
        Ok(())
    }
    fn close_stroke_fill(&mut self, fill_rule: FillRule) {
        self.current_contour.close();
        self.flush();
        self.graphics_state.draw(self.scene, &self.current_outline, DrawMode::FillStroke, fill_rule);
        self.current_outline.clear();
    }
    fn op_b(&mut self, ops: &OpArgs) -> Result<()> {
        // close, stroke and fill (winding)
        self.close_stroke_fill(FillRule::Winding);
        Ok(())
    }
    fn op_b_star(&mut self, ops: &OpArgs) -> Result<()> {
        // close, stroke and fill (winding)
        self.close_stroke_fill(FillRule::EvenOdd);
        Ok(())
    }
    fn op_n(&mut self, ops: &OpArgs) -> Result<()> {
        // clear path
        self.current_outline.clear();
        self.current_contour.clear();
        Ok(())
    }
    fn clip_path(&mut self, fill_rule: FillRule) {
        self.flush();
        let path = self.current_outline.clone().transformed(&self.graphics_state.transform);
        let mut clip_path = ClipPath::new(path);
        clip_path.set_fill_rule(fill_rule);
        let clip_path_id = self.scene.push_clip_path(clip_path);
        self.graphics_state.clip_path = Some(clip_path_id);
    }
    fn op_W(&mut self, ops: &OpArgs) -> Result<()> {
        // merge clip path (winding)
        self.clip_path(FillRule::Winding);
        Ok(())
    }
    fn op_W_star(&mut self, ops: &OpArgs) -> Result<()> {
        // merge clip path (even-odd)
        self.clip_path(FillRule::EvenOdd);
        Ok(())
    }
    fn op_q(&mut self, ops: &OpArgs) -> Result<()> {
        // save stack
        self.stack.push((self.graphics_state.clone(), self.text_state));
        Ok(())
    }
    fn op_Q(&mut self, ops: &OpArgs) -> Result<()> {
        // restore stack
        let (g, t) = self.stack.pop().expect("graphcs stack is empty");
        self.graphics_state = g;
        self.text_state = t;
        Ok(())
    }
    fn op_cm(&mut self, ops: &OpArgs) -> Result<()> {
        // modify transformation matrix 
        ops!(ops, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 => {
            self.graphics_state.transform = self.graphics_state.transform * Transform2F::row_major(a, c, e, b, d, f);
        });
        Ok(())
    }
    fn op_w(&mut self, ops: &OpArgs) -> Result<()> {
        // line width
        ops!(ops, width: f32 => {
            self.graphics_state.stroke_style.line_width = width;
        });
        Ok(())
    }
    fn op_gs(&mut self, ops: &OpArgs) -> Result<()> {
        ops!(ops, gs: &Primitive => { // set from graphic state dictionary
            let gs = gs.as_name()?;
            let gs = try_opt!(self.resources.graphics_states.get(gs));
            if let Some(lw) = gs.line_width {
                self.graphics_state.stroke_style.line_width = lw;
            }
            if let Some((ref font, size)) = gs.font {
                if let Some(e) = self.cache.get_font(&font.name) {
                    self.text_state.font_entry = Some(e);
                    self.text_state.font_size = size;
                    debug!("new font: {} at size {}", font.name, size);
                } else {
                    self.text_state.font_entry = None;
                }
            }
        });
        Ok(())
    }
    fn op_stroke_color(&mut self, ops: &OpArgs) -> Result<()> {
        // stroke color
        let paint = convert_color(self.graphics_state.stroke_color_space, &*ops)?;
        self.graphics_state.stroke_paint = self.scene.push_paint(&paint);
        Ok(())
    }
    fn op_fill_color(&mut self, ops: &OpArgs) -> Result<()> {
        // fill color
        let paint = convert_color(self.graphics_state.fill_color_space, &*ops)?;
        self.graphics_state.fill_paint = self.scene.push_paint(&paint);
        Ok(())
    }
    fn op_G(&mut self, ops: &OpArgs) -> Result<()> {
        // stroke gray
        ops!(ops, gray: f32 => {
            self.graphics_state.stroke_paint = self.scene.push_paint(&gray2fill(gray));
        });
        Ok(())
    }
    fn op_g(&mut self, ops: &OpArgs) -> Result<()> {
        // stroke gray
        ops!(ops, gray: f32 => {
            self.graphics_state.stroke_paint = self.scene.push_paint(&gray2fill(gray));
        });
        Ok(())
    }
    fn op_K(&mut self, ops: &OpArgs) -> Result<()> {
        // stroke color
        ops!(ops, c: f32, m: f32, y: f32, k: f32 => {
            self.graphics_state.stroke_paint = self.scene.push_paint(&cmyk2fill(c, m, y, k));
        });
        Ok(())
    }
    fn op_k(&mut self, ops: &OpArgs) -> Result<()> {
        // fill color
        ops!(ops, c: f32, m: f32, y: f32, k: f32 => {
            self.graphics_state.fill_paint = self.scene.push_paint(&cmyk2fill(c, m, y, k));
        });
        Ok(())
    }
    fn op_cs(&mut self, ops: &OpArgs) -> Result<()> {
        // fill color space
        ops!(ops, name: &Primitive => {
            let name = name.as_name()?;
            self.graphics_state.fill_color_space = self.resources.color_spaces.get(name).unwrap().clone();
        });
        Ok(())
    }
    fn op_CS(&mut self, ops: &OpArgs) -> Result<()> {
        // stroke color space
        ops!(ops, name: &Primitive => {
            let name = name.as_name()?;
            self.graphics_state.stroke_color_space = self.resources.color_spaces.get(name).unwrap().clone();
        });
        Ok(())
    }
    fn op_BT(&mut self, ops: &OpArgs) -> Result<()> {
        self.text_state.reset_matrix();
        Ok(())
    }
    fn op_ET(&mut self, ops: &OpArgs) -> Result<()> {
        Ok(())
    }
    fn op_Tc(&mut self, ops: &OpArgs) -> Result<()> {
        // character spacing
        ops!(ops, char_space: f32 => {
            self.text_state.char_space = char_space;
        });
        Ok(())
    }
    fn op_Tw(&mut self, ops: &OpArgs) -> Result<()> {
        // word spacing
        ops!(ops, word_space: f32 => {
            self.text_state.word_space = word_space;
        });
        Ok(())
    }
    fn op_Tz(&mut self, ops: &OpArgs) -> Result<()> {
        // Horizontal scaling (in percent)
        ops!(ops, scale: f32 => {
            self.text_state.horiz_scale = 0.01 * scale;
        });
        Ok(())
    }
    fn op_TL(&mut self, ops: &OpArgs) -> Result<()> {
        // leading
        ops!(ops, leading: f32 => {
            self.text_state.leading = leading;
        });
        Ok(())
    }
    fn op_Tf(&mut self, ops: &OpArgs) -> Result<()> {
        // text font
        ops!(ops, font_name: &Primitive, size: f32 => {
            let font_name = font_name.as_name()?;
            let font = try_opt!(self.resources.fonts.get(font_name));
            if let Some(e) = self.cache.get_font(&font.name) {
                self.text_state.font_entry = Some(e);
                debug!("new font: {} (is_cid={:?})", font.name, e.is_cid);
                self.text_state.font_size = size;
            } else {
                warn!("no font {}", font.name);
                self.text_state.font_entry = None;
            }
        });
        Ok(())
    }
    fn op_Tr(&mut self, ops: &OpArgs) -> Result<()> {
        // render mode
        ops!(ops, mode: i32 => {
            use TextMode::*;
            self.text_state.mode = match mode {
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
        });
        Ok(())
    }
    fn op_Ts(&mut self, ops: &OpArgs) -> Result<()> {
        // text rise
        ops!(ops, rise: f32 => {
            self.text_state.rise = rise;
        });
        Ok(())
    }
    fn op_Td(&mut self, ops: &OpArgs) -> Result<()> {
        // positioning operators
        // Move to the start of the next line
        ops_p!(ops, t => {
            self.text_state.translate(t);
        });
        Ok(())
    }
    fn op_TD(&mut self, ops: &OpArgs) -> Result<()> {
        ops_p!(ops, t => {
            self.text_state.leading = -t.y();
            self.text_state.translate(t);
        });
        Ok(())
    }
    fn op_Tm(&mut self, ops: &OpArgs) -> Result<()> {
        // Set the text matrix and the text line matrix
        ops!(ops, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 => {
            self.text_state.set_matrix(Transform2F::row_major(a, c, e, b, d, f));
        });
        Ok(())
    }
    fn op_T_star(&mut self, ops: &OpArgs) -> Result<()> {
        // Move to the start of the next line
        self.text_state.next_line();
        Ok(())
    }
    fn op_Tj(&mut self, ops: &OpArgs) -> Result<()> {
        // draw text
        ops!(ops, text: &[u8] => {
            let bb = self.text_state.draw_text(self.scene, &self.graphics_state, text);
            self.trace_bbox(bb);
        });
        Ok(())
    }
    fn op_tick(&mut self, ops: &OpArgs) -> Result<()> {
        // move to the next line and draw text
        ops!(ops, text: &[u8] => {
            self.text_state.next_line();
            let bb = self.text_state.draw_text(self.scene, &self.graphics_state, text);
            self.trace_bbox(bb);
        });
        Ok(())
    }
    fn op_doubletick(&mut self, ops: &OpArgs) -> Result<()> {
        // set word and charactr spacing, move to the next line and draw text
        ops!(ops, word_space: f32, char_space: f32, text: &[u8] => {
            self.text_state.word_space = word_space;
            self.text_state.char_space = char_space;
            self.text_state.next_line();
            let bb = self.text_state.draw_text(self.scene, &self.graphics_state, text);
            self.trace_bbox(bb);
        });
        Ok(())
    }
    fn op_TJ(&mut self, ops: &OpArgs) -> Result<()> {
        ops!(ops, array: &[Primitive] => {
            let mut bb = BBox::empty();
            for arg in array {
                match arg {
                    Primitive::String(ref data) => {
                        let r2 = self.text_state.draw_text(self.scene, &self.graphics_state, data.as_bytes());
                        bb.add_bbox(r2);
                    },
                    p => {
                        let offset = p.as_number().expect("wrong argument to TJ");
                        self.text_state.advance(-0.001 * offset); // because why not PDF…
                    }
                }
            }
            self.trace_bbox(bb);
        });
        Ok(())
    }
    fn draw_image(&mut self, name: &Primitive) -> Result<()> {
        let name = name.as_name()?;
        let &xobject_ref = self.resources.xobjects.get(name).unwrap();
        let xobject = self.file.get(xobject_ref)?;
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
                let outline = Outline::from_rect(self.graphics_state.transform * RectF::new(Vector2F::default(), Vector2F::new(1.0, 1.0)));
                let im_tr = self.graphics_state.transform
                    * Transform2F::from_scale(Vector2F::new(1.0 / size_f.x(), -1.0 / size_f.y()))
                    * Transform2F::from_translation(Vector2F::new(0.0, -size_f.y()));
                let mut pattern = Pattern::from_image(Image::new(size, Arc::new(data)));
                pattern.apply_transform(im_tr);
                let paint = Paint::from_pattern(pattern);
                let paint_id = self.scene.push_paint(&paint);
                let mut draw_path = DrawPath::new(outline, paint_id);
                draw_path.set_clip_path(self.graphics_state.clip_path);
                self.scene.push_draw_path(draw_path);

                self.trace_rect(self.graphics_state.transform * RectF::new(Vector2F::default(), size_f));
            },
            _ => {}
        }
        Ok(())
    }
    fn op_Do(&mut self, ops: &OpArgs) -> Result<()> {
        ops!(ops, name: &Primitive => {
            match self.draw_image(name) {
                Ok(()) => {},
                Err(e) => warn!("failed to decode image: {}", e)
            }
        });
        Ok(())
    }
    fn op_nop(&mut self, ops: &OpArgs) -> Result<()> {
        Ok(())
    }
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
