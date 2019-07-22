#[macro_use] extern crate log;
#[macro_use] extern crate slotmap;

use std::error::Error;
use pathfinder_canvas::Path2D;
use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::transform2d::Transform2F;
use std::fmt;
use nom::{IResult, Err::*, error::VerboseError};

pub struct Glyph {
    /// unit 1em
    pub width: f32,
    
    /// transform by font_matrix to scale it to 1em
    pub path: Path2D 
}

pub trait Font {
    fn num_glyphs(&self) -> u32;
    fn font_matrix(&self) -> Transform2F {
        Transform2F::row_major(1.0, 0., 0., 1., 0., 0.)
    }
    fn glyph(&self, id: u32) -> Result<Glyph, Box<dyn Error>>;
    fn glyphs(&self) -> Glyphs {
        Glyphs {
            glyphs: (0 .. self.num_glyphs()).map(|i| self.glyph(i).unwrap()).collect()
        }
    }
}

pub struct Glyphs {
    glyphs: Vec<Glyph>
}
impl Glyphs {
    pub fn get(&self, idx: u32) -> Option<&Glyph> {
        self.glyphs.get(idx as usize)
    }
}

mod truetype;
mod cff;
mod type1;
mod type2;
mod postscript;
mod parsers;

pub use truetype::TrueTypeFont;
pub use cff::CffFont;
pub use type1::Type1Font;

pub type R<'a, T> = IResult<&'a [u8], T, VerboseError<&'a [u8]>>;

#[derive(Copy, Clone)]
pub enum Value {
    Int(i32),
    Float(f32)
}
impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(i) => i.fmt(f),
            Value::Float(x) => x.fmt(f)
        }
    }
}

impl Into<f32> for Value {
    fn into(self) -> f32 {
        self.to_float()
    }
}
impl From<i16> for Value {
    fn from(v: i16) -> Value {
        Value::Int(v as i32)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Value {
        Value::Int(v)
    }
}
impl From<f32> for Value {
    fn from(v: f32) -> Value {
        Value::Float(v)
    }
}
impl Value {
    fn to_int(self) -> i32 {
        match self {
            Value::Int(i) => i,
            Value::Float(_) => panic!("tried to cast a float to int")
        }
    }
    fn to_uint(self) -> u32 {
        match self {
            Value::Int(i) if i >= 0 => i as u32,
            Value::Int(_) => panic!("expected a unsigned int"),
            Value::Float(_) => panic!("tried to cast a float to int")
        }
    }
    fn to_float(self) -> f32 {
        match self {
            Value::Int(i) => i as f32,
            Value::Float(f) => f
        }
    }
}

fn v(x: impl Into<f32>, y: impl Into<f32>) -> Vector2F {
    Vector2F::new(x.into(), y.into())
}

pub struct Context<'a> {
    pub global_subroutines: Vec<&'a [u8]>,
    pub private_subroutines: Vec<&'a [u8]>
}

fn bias(num: usize) -> i32 {
    if num < 1240 {
        107
    } else if num < 33900 {
        1131
    } else {
        32768
    }
}
impl<'a> Context<'a> {
    pub fn private_subroutine(&self, idx: i32) -> &'a [u8] {
        debug!("requesting {}", idx);
        let idx = idx + bias(self.private_subroutines.len());
        debug!("with bias {}", idx);
        self.private_subroutines.get(idx as usize).expect("requested subroutine not found")
    }
}
pub struct State {
    pub stack: Vec<Value>,
    pub path: Path2D,
    pub current: Vector2F,
    pub lsp: Option<Vector2F>,
    pub char_width: Option<f32>,
    pub done: bool,
    pub stem_hints: u32,
    pub delta_width: f32
}

impl State {
    pub fn new() -> State {
        State {
            stack: Vec::new(),
            path: Path2D::new(),
            current: Vector2F::new(0., 0.),
            lsp: None,
            char_width: None,
            done: false,
            stem_hints: 0,
            delta_width: 0.
        }
    }
    pub fn into_path(self) -> Path2D {
        self.path
    }
    pub fn push(&mut self, v: impl Into<Value>) {
        self.stack.push(v.into());
    }
    pub fn pop(&mut self) -> Value {
        self.stack.pop().expect("no value on the stack")
    }
}

pub trait IResultExt {
    type Item;
    fn get(self) -> Self::Item;
}
impl<T> IResultExt for IResult<&[u8], T, VerboseError<&[u8]>> {
    type Item = T;
    fn get(self) -> T {
        match self {
            Ok((_, t)) => t,
            Err(Incomplete(_)) => panic!("need more data"),
            Err(Error(v)) | Err(Failure(v)) => {
                for (i, e) in v.errors {
                    println!("{:?} {:?}", &i[.. i.len().min(20)], e);
                    println!("{:?}", String::from_utf8_lossy(&i[.. i.len().min(20)]));
                }
                panic!()
            }
        }
    }
}
