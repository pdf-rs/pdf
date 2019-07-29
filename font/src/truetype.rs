use std::error::Error;
use std::iter;
use std::collections::HashMap;
use pathfinder_canvas::Path2D;
use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::transform2d::{Transform2F, Matrix2x2F};
use crate::{Font, BorrowedFont, Glyph, R, IResultExt};
use encoding::Encoding;
use nom::{
    number::complete::{be_u8, be_i8, be_i16, be_u16},
    bytes::complete::take,
    sequence::tuple,
    multi::{count},
};
use crate::opentype::{parse_head, parse_maxp, parse_loca, parse_cmap, parse_hhea, parse_hmtx, Hmtx, Tables};

#[derive(Clone)]
enum Shape {
    Simple(Path2D),
    Compound(Vec<(u32, Transform2F)>),
    Empty
}
pub struct TrueTypeFont<'a> {
    data: &'a [u8],
    shapes: Vec<Option<Shape>>,
    loca: Vec<u32>,
    cmap: Option<HashMap<u32, u16>>,
    hmtx: Option<Hmtx<'a>>,
    units_per_em: u16
}
impl<'a> TrueTypeFont<'a> {
    pub fn parse_glyf(data: &'a [u8], tables: Tables<'a>) -> Self {
        let head = parse_head(tables.get(b"head").expect("no head")).get();
        let maxp = parse_maxp(tables.get(b"maxp").expect("no maxp")).get();
        let loca = parse_loca(tables.get(b"loca").expect("no loca"), &head, &maxp).get();
        let cmap = tables.get(b"cmap").map(|data| parse_cmap(data).get());
        let hhea = tables.get(b"hhea").map(|data| parse_hhea(data).get());
        let hmtx = match (hhea, tables.get(b"hmtx")) {
            (Some(hhea), Some(data)) => Some(parse_hmtx(data, &hhea, &maxp)),
            _ => None
        };
        let num_glyphs = maxp.num_glyphs;
        TrueTypeFont {
            data,
            shapes: vec![None; num_glyphs as usize],
            loca: loca,
            cmap,
            hmtx,
            units_per_em: head.units_per_em
        }
    }
    fn get_glyph_data(&self, idx: usize) -> &'a [u8] {
        let start = self.loca[idx];
        let end = self.loca[idx + 1];
        self.data.get(start as usize .. end as usize).unwrap()
    }
    fn get_shape(&self, idx: usize) -> Shape {
        debug!("get shape for glyph {}", idx);
        let idx = idx as usize;
        let data = self.get_glyph_data(idx);
        parse_glyph_shape(data).get()
    }
    fn get_path(&self, idx: u32) -> Path2D {
        let shape = self.get_shape(idx as usize);
    
        match shape {
            Shape::Simple(path) => path,
            Shape::Compound(parts) => {
                let mut path = Path2D::new();
                for (gid, tr) in parts {
                    path.add_path(self.get_path(gid).transform(&tr));
                }
                path
            }
            Shape::Empty => Path2D::new()
        }
    }
}
impl<'a> Font for TrueTypeFont<'a> {
    fn num_glyphs(&self) -> u32 {
        self.shapes.len() as u32
    }
    fn font_matrix(&self) -> Transform2F {
        let scale = 1.0 / self.units_per_em as f32;
        Transform2F::from_scale(Vector2F::new(scale, scale))
    }
    fn glyph(&self, id: u32) -> Result<Glyph, Box<dyn Error>> {
        assert!(id <= u16::max_value() as u32);
        let path = self.get_path(id);
        let width = self.hmtx.as_ref().map(|hmtx| hmtx.metrics_for_gid(id as u16).advance).unwrap_or(0);
        
        Ok(Glyph {
            path,
            width: width as f32
        })
    }
    fn gid_for_unicode_codepoint(&self, codepoint: u32) -> Option<u32> {
        match self.cmap {
            Some(ref cmap) => cmap.get(&codepoint).map(|&gid| gid as u32),
            None => None
        }
    }
    fn encoding(&self) -> Option<Encoding> {
        Some(Encoding::Unicode)
    }
}

impl<'a> BorrowedFont<'a> for TrueTypeFont<'a> {}

#[inline]
fn vec_i8(i: &[u8]) -> R<Vector2F> {
    let (i, x) = be_i8(i)?;
    let (i, y) = be_i8(i)?;
    Ok((i, Vector2F::new(x as f32, y as f32)))
}
#[inline]
fn vec_i16(i: &[u8]) -> R<Vector2F> {
    let (i, x) = be_i16(i)?;
    let (i, y) = be_i16(i)?;
    Ok((i, Vector2F::new(x as f32, y as f32)))
}
#[inline]
fn fraction_i16(i: &[u8]) -> R<f32> {
    let (i, s) = be_i16(i)?;
    Ok((i, s as f32 / 16384.0))
}
#[inline]
fn mid(a: Vector2F, b: Vector2F) -> Vector2F {
    (a + b) * Vector2F::new(0.5, 0.5)
}

// the following code is borrowed from stb-truetype and modified to fit pathfinder



fn parse_glyph_shape(data: &[u8]) -> R<Shape> {
    if data.len() == 0 {
        return Ok((data, Shape::Empty));
    }
    let (i, number_of_contours) = be_i16(data)?;
    dbg!(number_of_contours);
    
    let (i, _) = take(8usize)(i)?;
    match number_of_contours {
        0 => Ok((i, Shape::Empty)),
        n if n >= 0 => glyph_shape_positive_contours(i, number_of_contours as usize),
        -1 => {
            // Compound shapes
            let mut more = true;
            let mut parts = Vec::new();
            let mut input = i;
            while more {
                let i = input;
                let mut transform = Transform2F::default();

                let (i, flags) = be_i16(i)?;
                let (i, gidx) = be_u16(i)?;

                let i = if flags & 2 != 0 {
                    // XY values
                    let (i, translation) = if flags & 1 != 0 {
                        // shorts
                        vec_i16(i)?
                    } else {
                        vec_i8(i)?
                    };
                    transform.vector = translation;
                    i
                } else {
                    panic!("Matching points not supported.");
                };
                let i = if flags & (1 << 3) != 0 {
                    // WE_HAVE_A_SCALE
                    let (i, scale) = fraction_i16(i)?;
                    transform.matrix = Matrix2x2F::from_scale(Vector2F::new(scale, scale));
                    i
                } else if flags & (1 << 6) != 0 {
                    // WE_HAVE_AN_X_AND_YSCALE
                    let (i, (sx, sy)) = tuple((fraction_i16, fraction_i16))(i)?;
                    transform.matrix = Matrix2x2F::from_scale(Vector2F::new(sx, sy));
                    i
                } else if flags & (1 << 7) != 0 {
                    // WE_HAVE_A_TWO_BY_TWO
                    let (i, (a, b, c, d)) = tuple((fraction_i16, fraction_i16, fraction_i16, fraction_i16))(i)?;
                    transform.matrix = Matrix2x2F::row_major(a, b, c, d);
                    i
                } else {
                    i
                };

                // Get indexed glyph.
                parts.push((gidx as u32, transform));
                // More components ?
                more = flags & (1 << 5) != 0;
                input = i;
            }
            Ok((input, Shape::Compound(parts)))
        }
        n => panic!("Contour format {} not supported.", n)
    }
}

#[derive(Copy, Clone, Debug)]
struct FlagData {
    flags: u8,
    p: Vector2F
}

fn glyph_shape_positive_contours(i: &[u8], number_of_contours: usize) -> R<Shape> {
    let delta2 = move |j: &[u8]| j.as_ptr() as usize - i.as_ptr() as usize;
    let delta = move |j: &[u8], msg: &'static str| debug!("{} @ {}", msg, j.as_ptr() as usize - i.as_ptr() as usize);
    
    let mut path = Path2D::new();
    let mut start_off = false;
    let mut was_off = false;
    
    delta(i, "point_indices");
    // let end_points_of_contours = &glyph_data[10..];
    let (i, point_indices) = count(be_u16, number_of_contours)(i)?;
    
    // let ins = read_u16(&glyph_data[10 + number_of_contours * 2..]) as usize;
    let (i, num_instructions) = be_u16(i)?;
    let (i, _instructions) = take(num_instructions)(i)?;
    // 
    // let mut points = &glyph_data[10 + number_of_contours * 2 + 2 + ins ..];

    // let n = 1 + read_u16(&end_points_of_contours[number_of_contours * 2 - 2..]) as usize;
    let n = 1 + *point_indices.last().unwrap() as usize;
    debug!("n={}", n);

    let mut flag_data = Vec::with_capacity(n);


    // in first pass, we load uninterpreted data into the allocated array above

    // first load flags
    delta(i, "flags");
    let mut input = i;
    while flag_data.len() < n {
        let (i, flags) = be_u8(input)?;
        let flag = FlagData { flags, p: Vector2F::default() };
        
        if flags & 8 != 0 {
            let (i, flagcount) = be_u8(i)?;
            let num = (n - flag_data.len()).min(flagcount as usize + 1);
            flag_data.extend(iter::repeat(flag).take(num));
            input = i;
        } else {
            flag_data.push(flag);
            input = i;
        }
    }
    assert_eq!(flag_data.len(), n);

    fn parse_coord(i: &[u8], short: bool, same_or_pos: bool) -> R<i16> {
        match (short, same_or_pos) {
            (true, true) => {
                let (i, dx) = be_u8(i)?;
                Ok((i, dx as i16))
            }
            (true, false) => {
                let (i, dx) = be_u8(i)?;
                Ok((i, - (dx as i16)))
            }
            (false, false) => {
                let (i, dx) = be_i16(i)?;
                Ok((i, dx))
            }
            (false, true) => Ok((i, 0))
        }
    }
    
    delta(input, "flag x data");
    // now load x coordinates
    let mut x_coord = 0;
    for (j, &mut FlagData { flags, ref mut p }) in flag_data.iter_mut().enumerate() {
        let (i, dx) = parse_coord(input, flags & 2 != 0, flags & 16 != 0)?;
        x_coord += dx;
        p.set_x(x_coord as f32);
        input = i;
        debug!("{}: flags={} {:?} @ {}", j, flags, p, delta2(input));
    }

    delta(input, "flag y data");
    // now load y coordinates
    let mut y_coord = 0;
    for (j, &mut FlagData { flags, ref mut p }) in flag_data.iter_mut().enumerate() {
        let (i, dy) = parse_coord(input, flags & 4 != 0, flags & 32 != 0)?;
        y_coord += dy;
        p.set_y(y_coord as f32);
        input = i;
        debug!("{}: flags={} {:?} @ {}", j, flags, p, delta2(input));
    }

    // now convert them to our format
    let mut s = Vector2F::new(0., 0.);
    let mut c = Vector2F::new(0., 0.);
    let mut sc = Vector2F::new(0., 0.);
    let mut j = 0;

    let mut iter = flag_data.into_iter().enumerate().peekable();

    let mut next_move = 0;
    while let Some((index, FlagData { flags, p })) = iter.next() {
        if next_move == index {
            if index != 0 {
                close_shape(&mut path, was_off, start_off, c, sc, s);
            }

            // now start the new one
            start_off = flags & 1 == 0;
            if start_off {
                // if we start off with an off-curve point, then when we need to find a
                // point on the curve where we can start, and we
                // need to save some state for
                // when we wraparound.
                sc = p;

                let (next_flags, next) = match iter.peek() {
                    Some((_, fd)) => (fd.flags, fd.p),
                    None => break,
                };

                if next_flags & 1 == 0 {
                    // next point is also a curve point, so interpolate an on-point curve
                    s = mid(p, next);
                } else {
                    // otherwise just use the next point as our start point
                    s = next;

                    // we're using point i+1 as the starting point, so skip it
                    let _ = iter.next();
                }
            } else {
                s = p;
            }
            path.move_to(s);
            was_off = false;
            next_move = 1 + point_indices[j] as usize;
            j += 1;
        } else if flags & 1 == 0 {
            // if it's a curve
            if was_off {
                // two off-curve control points in a row means interpolate an on-curve
                // midpoint
                path.quadratic_curve_to(c, mid(c, p));
            }
            c = p;
            was_off = true;
        } else {
            if was_off {
                path.quadratic_curve_to(c, p);
            } else {
                path.line_to(p);
            }
            was_off = false;
        }
    }
    close_shape(&mut path, was_off, start_off, c, sc, s);
    path.close_path();
    
    Ok((input, Shape::Simple(path)))
}

#[inline]
fn close_shape(path: &mut Path2D, was_off: bool, start_off: bool, c: Vector2F, sc: Vector2F, s: Vector2F) {
    if start_off {
        if was_off {
            path.quadratic_curve_to(c, mid(c, sc));
        }
        path.quadratic_curve_to(sc, s);
    } else {
        if was_off {
            path.quadratic_curve_to(c, s);
        } else {
            path.line_to(s);
        }
    }
}

