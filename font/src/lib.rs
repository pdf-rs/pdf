#[macro_use] extern crate log;
#[macro_use] extern crate slotmap;

use std::error::Error;
use pathfinder_canvas::Path2D;
use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::transform2d::Transform2F;
use std::fmt;
use std::borrow::Cow;
use std::path::Path;
use nom::{IResult, Err::*, error::VerboseError};
use tuple::{TupleElements, Map};

#[derive(Clone)]
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
    fn glyph(&self, gid: u32) -> Result<Glyph, Box<dyn Error>>;
    fn glyphs(&self) -> Glyphs {
        Glyphs {
            glyphs: (0 .. self.num_glyphs()).map(|i| self.glyph(i).unwrap()).collect()
        }
    }
    fn gid_for_codepoint(&self, codepoint: u32) -> Option<u32>;
    fn gid_for_name(&self, name: &str) -> Option<u32>;
}

pub struct Glyphs {
    glyphs: Vec<Glyph>
}
impl Glyphs {
    pub fn get(&self, codepoint: u32) -> Option<&Glyph> {
        self.glyphs.get(codepoint as usize)
    }
}

mod truetype;
mod cff;
mod type1;
mod type2;
mod postscript;
mod opentype;
mod parsers;
mod eexec;

pub use truetype::TrueTypeFont;
pub use cff::CffFont;
pub use type1::Type1Font;
pub use opentype::parse_opentype;

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
    pub subr_bias: i32,
    pub subrs: Vec<Cow<'a, [u8]>>,
    pub global_subrs: Vec<Cow<'a, [u8]>>,
    pub global_subr_bias: i32,
}

impl<'a> Context<'a> {
    pub fn subr(&self, idx: i32) -> &[u8] {
        self.subrs.get((idx + self.subr_bias) as usize).expect("requested subroutine not found")
    }
    pub fn global_subr(&self, idx: i32) -> &[u8] {
        self.global_subrs.get((idx + self.global_subr_bias) as usize).expect("requested global subroutine not found")
    }
}
pub struct State {
    pub stack: Vec<Value>,
    pub path: Path2D,
    pub current: Vector2F,
    pub lsb: Option<Vector2F>,
    pub char_width: Option<f32>,
    pub done: bool,
    pub stem_hints: u32,
    pub delta_width: Option<f32>
}

impl State {
    pub fn new() -> State {
        State {
            stack: Vec::new(),
            path: Path2D::new(),
            current: Vector2F::new(0., 0.),
            lsb: None,
            char_width: None,
            done: false,
            stem_hints: 0,
            delta_width: None
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
    /// get stack[0 .. T::N] as a tuple
    /// does not modify the stack
    pub fn args<T>(&mut self) -> T where
        T: TupleElements<Element=Value>
    {
        debug!("get {} args from {:?}", T::N, self.stack);
        T::from_iter(self.stack.iter().cloned()).unwrap()
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

pub trait BorrowedFont<'a>: Font {}
/*
pub struct Owned<F>
{
    container: Pin<Box<u8>>,
    font: F
}
impl<F> Owned<F> where F: for <'a> BorrowedFont<'a> {
    pub fn new(data: Box<u8>, f: impl FnOnce(Pin<&[u8]>) -> F) -> Owned<C, F> {
        let container = data.into_pin();
        let font = f(container.as_ref());
        Owned { container, font }
    }
}

impl<T> Font for Owned<T> where T: for <'a> BorrowedFont<'a> {
    fn num_glyphs(&self) -> u32 {
        self.font.num_glyphs()
    }
    fn font_matrix(&self) -> Transform2F {
        self.font.font_matrix()
    }
    fn glyph(&self, id: u32) -> Result<Glyph, Box<dyn Error>> {
        self.font.glyph(id)
    }
    fn glyphs(&self) -> Glyphs {
        self.font.glyphs()
    }
    fn gid_for_codepoint(&self, codepoint: u32) -> Option<u32> {
        self.font.gid_for_codepoint(codepoint)
    }
    fn gid_for_name(&self, name: &str) -> Option<u32> {
        self.font.gid_for_name(name)
    }
}

pub type OwnedFont = 
pub fn parse_file(path: &Path) -> OwnedFont {
    let data: Vec<u8> = std::fs::read(path).unwrap();
    Owned::new(data.into_box(), |data| parse(data))
}
*/
pub fn parse<'a>(data: &'a [u8]) -> Box<dyn BorrowedFont<'a> + 'a> {
    let mut magic = [0; 4];
    magic.copy_from_slice(&data[0 .. 4]);
    match &magic {
        &[1, _, _, _] => Box::new(CffFont::parse(data, 0)) as _,
        &[0x80, 1, _, _] => Box::new(Type1Font::parse_pfb(data)) as _,
        b"OTTO" => Box::new(parse_opentype(data, 0)) as _,
        b"ttcf" | b"typ1" | [1,0,0,0] | [0,1,0,0] => Box::new(TrueTypeFont::parse(data)) as _,
        magic => panic!("unknown magic {:?}", magic)
    }
}
