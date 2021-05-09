/// PDF content streams.
use std::fmt::{Display, Formatter};
use std::mem::replace;
use std::cmp::Ordering;
use std::io;
use itertools::Itertools;

use crate::error::*;
use crate::object::*;
use crate::parser::{Lexer, parse_with_lexer};
use crate::primitive::*;

/// Operation in a PDF content stream.
#[derive(Debug, Clone)]
pub struct Operation {
    pub operator: String,
    pub operands: Vec<Primitive>,
}

impl Operation {
    pub fn new(operator: impl Into<String>, operands: Vec<Primitive>) -> Operation {
        Operation{
            operator: operator.into(),
            operands,
        }
    }
}


/// Represents a PDF content stream - a `Vec` of `Operator`s
#[derive(Debug, Clone, Default)]
pub struct Content {
    pub operations: Vec<Op>,
}

macro_rules! names {
    ($args:ident, $($x:ident),*) => (
        $(
            let $x = name(&mut $args)?;
        )*
    )
}
macro_rules! numbers {
    ($args:ident, $($x:ident),*) => (
        $(
            let $x = number(&mut $args)?;
        )*
    )
}
macro_rules! points {
    ($args:ident, $($point:ident),*) => (
        $(
            let $point = point(&mut $args)?;
        )*
    )
}
fn name(args: &mut impl Iterator<Item=Primitive>) -> Result<String> {
    args.next().ok_or(PdfError::NoOpArg)?.into_name()
}
fn number(args: &mut impl Iterator<Item=Primitive>) -> Result<f32> {
    args.next().ok_or(PdfError::NoOpArg)?.as_number()
}
fn string(args: &mut impl Iterator<Item=Primitive>) -> Result<PdfString> {
    args.next().ok_or(PdfError::NoOpArg)?.into_string()
}
fn point(args: &mut impl Iterator<Item=Primitive>) -> Result<Point> {
    let x = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let y = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    Ok(Point { x, y })
}
fn rect(args: &mut impl Iterator<Item=Primitive>) -> Result<Rect> {
    let x = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let y = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let width = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let height = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    Ok(Rect { x, y, width, height })
}
fn rgb(args: &mut impl Iterator<Item=Primitive>) -> Result<Rgb> {
    let red = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let green = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let blue = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    Ok(Rgb { red, green, blue })
}
fn cmyk(args: &mut impl Iterator<Item=Primitive>) -> Result<Cmyk> {
    let cyan = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let magenta = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let yellow = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let key = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    Ok(Cmyk { cyan, magenta, yellow, key })
}
fn number_list(args: &mut impl Iterator<Item=Primitive>) -> Result<Box<[f32]>> {
    Ok(args.map(|p| p.as_number()).collect::<Result<Vec<f32>, PdfError>>()?.into())
}
fn number_list_and_maybe_name(args: &mut impl Iterator<Item=Primitive>) -> Result<(Box<[f32]>, Option<String>)> {
    let mut list = Vec::new();
    for p in args {
        match p {
            Primitive::Number(f) => list.push(f),
            Primitive::Integer(i) => list.push(i as f32),
            Primitive::Name(name) => return Ok((list.into(), Some(name))),
            _ => bail!("invalid arguments")
        }
    }
    Ok((list.into(), None))
}
fn matrix(args: &mut impl Iterator<Item=Primitive>) -> Result<Matrix> {
    Ok(Matrix {
        a: number(args)?,
        b: number(args)?,
        c: number(args)?,
        d: number(args)?,
        e: number(args)?,
        f: number(args)?,
    })
}

struct OpBuilder {
    last: Point,
    compability_section: bool,
    ops: Vec<Op>
}
impl OpBuilder {
    fn new() -> Self {
        OpBuilder {
            last: Point { x: 0., y: 0. },
            compability_section: false,
            ops: Vec::new()
        }
    }
    fn parse(&mut self, data: &[u8], resolve: &impl Resolve) -> Result<()> {
        let mut lexer = Lexer::new(data);
        let mut buffer = Vec::with_capacity(5);

        loop {
            let backup_pos = lexer.get_pos();
            let obj = parse_with_lexer(&mut lexer, resolve);
            match obj {
                Ok(obj) => {
                    // Operand
                    buffer.push(obj)
                }
                Err(e) => {
                    if e.is_eof() {
                        break;
                    }
                    // It's not an object/operand - treat it as an operator.
                    lexer.set_pos(backup_pos);
                    let op = t!(lexer.next());
                    let operator = t!(op.as_str());
                    t!(self.add(operator, buffer.drain(..)));
                }
            }
            match lexer.get_pos().cmp(&data.len()) {
                Ordering::Greater => err!(PdfError::ContentReadPastBoundary),
                Ordering::Less => (),
                Ordering::Equal => break
            }
        }
        Ok(())
    }
    fn add(&mut self, op: &str, mut args: impl Iterator<Item=Primitive>) -> Result<()> {
        use Winding::*;

        let ops = &mut self.ops;
        let mut push = move |op| ops.push(op);

        match op {
            "b"   => push(Op::FillAndStroke { close: true, winding: NonZero }),
            "B"   => push(Op::FillAndStroke { close: false, winding: NonZero }),
            "b*"  => push(Op::FillAndStroke { close: true, winding: EvenOdd }),
            "B*"  => push(Op::FillAndStroke { close: false, winding: EvenOdd }),
            "BDC" => push(Op::BeginMarkedContent {
                tag: name(&mut args)?,
                properties: Some(args.next().ok_or(PdfError::NoOpArg)?)
            }),
            "BI"  => unimplemented!(),
            "BMC" => push(Op::BeginMarkedContent {
                tag: name(&mut args)?,
                properties: None
            }),
            "BT"  => push(Op::BeginText),
            "BX"  => self.compability_section = true,
            "c"   => {
                points!(args, c1, c2, p);
                push(Op::CurveTo { c1, c2, p });
                self.last = p;
            }
            "cm"  => {
                numbers!(args, a, b, c, d, e, f);
                push(Op::Transform { matrix: Matrix { a, b, c, d, e, f }});
            }
            "CS"  => {
                names!(args, name);
                push(Op::StrokeColorSpace { name });
            }
            "cs"  => {
                names!(args, name);
                push(Op::FillColorSpace { name });
            }
            "d"  => {
                let p = args.next().ok_or(PdfError::NoOpArg)?;
                let pattern = p.as_array()?.iter().map(|p| p.as_number()).collect::<Result<Vec<f32>, PdfError>>()?;
                let phase = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
                push(Op::Dash { pattern, phase });
            }
            "d0"  => {}
            "d1"  => {}
            "Do"  => {
                names!(args, name);
                push(Op::XObject { name });
            }
            "DP"  => push(Op::MarkedContentPoint {
                tag: name(&mut args)?,
                properties: Some(args.next().ok_or(PdfError::NoOpArg)?)
            }),
            "EI"  => unimplemented!(),
            "EMC" => push(Op::EndMarkedContent),
            "ET"  => push(Op::EndText),
            "EX"  => self.compability_section = false,
            "f" |
            "F"   => push(Op::Fill { close: false, winding: NonZero }),
            "f*"  => push(Op::Fill { close: false, winding: EvenOdd }),
            "G"   => push(Op::StrokeColor { color: Color::Gray(number(&mut args)?) }),
            "g"   => push(Op::FillColor { color: Color::Gray(number(&mut args)?) }),
            "gs"  => push(Op::GraphicsState { name: name(&mut args)? }),
            "h"   => push(Op::Close),
            "i"   => push(Op::Flatness { tolerance: number(&mut args)? }),
            "ID"  => unimplemented!(),
            "j"   => {
                let n = args.next().ok_or(PdfError::NoOpArg)?.as_integer()?;
                let join = match n {
                    0 => LineJoin::Miter,
                    1 => LineJoin::Round,
                    2 => LineJoin::Bevel,
                    _ => bail!("invalid line join {}", n)
                };
                push(Op::LineJoin { join });
            }
            "J"   => {
                let n = args.next().ok_or(PdfError::NoOpArg)?.as_integer()?;
                let cap = match n {
                    0 => LineCap::Butt,
                    1 => LineCap::Round,
                    2 => LineCap::Square,
                    _ => bail!("invalid line cap {}", n)
                };
                push(Op::LineCap { cap });
            }
            "K"   => {
                let color = Color::Cmyk(cmyk(&mut args)?);
                push(Op::StrokeColor { color });
            }
            "k"   => {
                let color = Color::Cmyk(cmyk(&mut args)?);
                push(Op::FillColor { color });
            }
            "l"   => {
                let p = point(&mut args)?;
                push(Op::LineTo { p });
                self.last = p;
            }
            "m"   => {
                let p = point(&mut args)?;
                push(Op::MoveTo { p });
                self.last = p;
            }
            "M"   => push(Op::MiterLimit { limit: number(&mut args)? }),
            "MP"  => push(Op::MarkedContentPoint { tag: name(&mut args)?, properties: None }),
            "n"   => push(Op::EndPath),
            "q"   => push(Op::Save),
            "Q"   => push(Op::Restore),
            "re"  => push(Op::Rect { rect: rect(&mut args)? }),
            "RG"  => push(Op::StrokeColor { color: Color::Rgb(rgb(&mut args)?) }),
            "rg"  => push(Op::FillColor { color: Color::Rgb(rgb(&mut args)?) }),
            "ri"  => {
                let intent = match args.next().ok_or(PdfError::NoOpArg)?.as_name()? {
                    "AbsoluteColorimetric" => RenderingIntent::AbsoluteColorimetric,
                    "RelativeColorimetric" => RenderingIntent::RelativeColorimetric,
                    "Perceptual" => RenderingIntent::Perceptual,
                    "Saturation" => RenderingIntent::Saturation,
                    s => bail!("invalid rendering intent {}", s)
                };
                push(Op::RenderingIntent { intent });
            },
            "s"   => push(Op::Stroke { close: true }),
            "S"   => push(Op::Stroke { close: false }),
            "SC"  => {
                let list = number_list(&mut args)?;
                push(Op::StrokeColor { color: Color::Other(list, None) });
            }
            "sc"  => {
                let list = number_list(&mut args)?;
                push(Op::FillColor { color: Color::Other(list, None) });
            }
            "SCN" => {
                let (list, name) = number_list_and_maybe_name(&mut args)?;
                push(Op::StrokeColor { color: Color::Other(list, name) });
            }
            "scn" => {
                let (list, name) = number_list_and_maybe_name(&mut args)?;
                push(Op::FillColor { color: Color::Other(list, name) });
            }
            "sh"  => {

            }
            "T*"  => push(Op::TextNewline),
            "Tc"  => push(Op::CharSpacing { char_space: number(&mut args)? }),
            "Td"  => push(Op::TextPosition { translation: point(&mut args)? }),
            "TD"  => {
                let translation = point(&mut args)?;
                push(Op::Leading { leading: -translation.x });
                push(Op::TextPosition { translation });
            }
            "Tf"  => push(Op::TextFont { name: name(&mut args)?, size: number(&mut args)? }),
            "Tj"  => push(Op::TextDraw { data: string(&mut args)? }),
            "TJ"  => push(Op::TextDrawAdjusted { array: args.collect() }),
            "TL"  => push(Op::Leading { leading: number(&mut args)? }),
            "Tm"  => push(Op::TextMatrix { matrix: matrix(&mut args)? }), 
            "Tr"  => {
                use TextMode::*;

                let n = args.next().ok_or(PdfError::NoOpArg)?.as_integer()?;
                let mode = match n {
                    0 => Fill,
                    1 => Stroke,
                    2 => FillThenStroke,
                    3 => Invisible,
                    4 => FillAndClip,
                    5 => StrokeAndClip,
                    _ => {
                        bail!("Invalid text render mode: {}", n);
                    }
                };
                push(Op::TextRenderMode { mode });
            }
            "Ts"  => push(Op::TextRise { rise: number(&mut args)? }),
            "Tw"  => push(Op::WordSpacing { word_space: number(&mut args)? }),
            "Tz"  => push(Op::TextScaling { horiz_scale: number(&mut args)? }),
            "v"   => {
                points!(args, c2, p);
                push(Op::CurveTo { c1: self.last, c2, p });
                self.last = p;
            }
            "w"   => push(Op::LineWidth { width: number(&mut args)? }),
            "W"   => push(Op::Clip { winding: NonZero }),
            "W*"  => push(Op::Clip { winding: EvenOdd }),
            "y"   => {
                points!(args, c1, p);
                push(Op::CurveTo { c1, c2: p, p });
                self.last = p;
            }
            "'"   => {
                push(Op::TextNewline);
                push(Op::TextDraw { data: string(&mut args)? });
            }
            "\""  => {
                push(Op::WordSpacing { word_space: number(&mut args)? });
                push(Op::CharSpacing { char_space: number(&mut args)? });
                push(Op::TextDraw { data: string(&mut args)? });
            }
            o if !self.compability_section => {
                bail!("invalid operator {}", o)
            },
            _ => {}
        }
        Ok(())
    }
}

impl Content {
}

impl Object for Content {
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        type ContentStream = Stream<()>;
        let mut ops = OpBuilder::new();
        
        match p {
            Primitive::Array(parts) => {
                for p in parts {
                    let part = t!(ContentStream::from_primitive(p, resolve));
                    let data = t!(part.data());
                    ops.parse(&data, resolve)?;
                }
            }
            p => {
                ops.parse(
                    t!(t!(ContentStream::from_primitive(p, resolve)).data()),
                    resolve
                )?;
            }
        }

        Ok(Content { operations: ops.ops })
    }
}

impl ObjectWrite for Content {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        use std::io::Write;

        let mut data = Vec::new();
        let f = &mut data;
        
        for operation in &self.operations {
            match operation {
                Op::BeginMarkedContent { tag, properties: Some(ref name) } => writeln!(f, "/{} /{} BDC", tag, name)?,
                Op::BeginMarkedContent { tag, properties: None } => writeln!(f, "/{} BMC", tag)?,
                Op::EndMarkedContent => writeln!(f, "EMC")?,
                Op::BeginText => writeln!(f, "BT")?,
                Op::EndText => writeln!(f, "ET")?,
                _ => unimplemented!()
            }
        }

        Stream::new((), data).to_primitive(update)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Winding {
    EvenOdd,
    NonZero
}

#[derive(Debug, Copy, Clone)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(8))]
pub struct Point {
    pub x: f32,
    pub y: f32
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(8))]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(8))]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

#[derive(Debug, Clone)]
pub enum Color {
    Indexed(i32),
    Gray(f32),
    Rgb(Rgb),
    Cmyk(Cmyk),
    Other(Box<[f32]>, Option<String>),
}

#[derive(Debug, Copy, Clone)]
pub enum TextMode {
    Fill,
    Stroke,
    FillThenStroke,
    Invisible,
    FillAndClip,
    StrokeAndClip
}

#[derive(Debug, Copy, Clone)]
pub struct Rgb {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

#[derive(Debug, Copy, Clone)]
pub struct Cmyk {
    pub cyan: f32,
    pub magenta: f32,
    pub yellow: f32,
    pub key: f32,
}

/// Graphics Operator
/// 
/// See PDF32000 A.2
#[derive(Debug, Clone)]
pub enum Op {
    /// Begin a marked comtent sequence
    /// 
    /// Pairs with the following EndMarkedContent.
    /// 
    /// generated by operators `BMC` and `BDC`
    BeginMarkedContent { tag: String, properties: Option<Primitive> },

    /// End a marked content sequence.
    /// 
    /// Pairs with the previous BeginMarkedContent.
    /// 
    /// generated by operator `EMC`
    EndMarkedContent,

    /// A marked content point.
    /// 
    /// generated by operators `MP` and `DP`.
    MarkedContentPoint { tag: String, properties: Option<Primitive> },


    Close,
    MoveTo { p: Point },
    LineTo { p: Point },
    CurveTo { c1: Point, c2: Point, p: Point },
    Rect { rect: Rect },
    EndPath,

    Stroke { close: bool },

    StrokeAndFill { close: bool, winding: Winding },

    /// Fill and Stroke operation
    /// 
    /// generated by operators `b`, `B`, `b*`, `B*`
    FillAndStroke { close: bool, winding: Winding },

    Fill { close: bool, winding: Winding },

    /// Fill using the named shading pattern
    /// 
    /// operator: `sh`
    Shade { name: String },

    Clear,

    Clip { winding: Winding },

    Save,
    Restore,

    Transform { matrix: Matrix },

    LineWidth { width: f32 },
    Dash { pattern: Vec<f32>, phase: f32 },
    LineJoin { join: LineJoin },
    LineCap { cap: LineCap },
    MiterLimit { limit: f32 },
    Flatness { tolerance: f32 },

    GraphicsState { name: String },

    StrokeColor { color: Color },
    FillColor { color: Color },

    FillColorSpace { name: String },
    StrokeColorSpace { name: String },

    RenderingIntent { intent: RenderingIntent },

    BeginText,
    EndText,

    CharSpacing { char_space: f32 },
    WordSpacing { word_space: f32 },
    TextScaling { horiz_scale: f32 },
    Leading { leading: f32 },
    TextFont { name: String, size: f32 },
    TextRenderMode { mode: TextMode },

    /// `Ts`
    TextRise { rise: f32 },

    /// `Td`, `TD`
    TextPosition { translation: Point },

    /// `Tm`
    TextMatrix { matrix: Matrix },

    /// `T*`
    TextNewline,

    /// `Tj`
    TextDraw { data: PdfString },

    TextDrawAdjusted { array: Vec<Primitive> },

    XObject { name: String },
}