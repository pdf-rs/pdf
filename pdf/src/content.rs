/// PDF content streams.
use std::fmt::{self, Display};
use std::cmp::Ordering;
use itertools::Itertools;
use istring::SmallString;
use datasize::DataSize;
use std::sync::Arc;

use crate::error::*;
use crate::object::*;
use crate::parser::{Lexer, parse_with_lexer, ParseFlags};
use crate::primitive::*;
use crate::enc::StreamFilter;
use crate as pdf;

/// Represents a PDF content stream - a `Vec` of `Operator`s
#[derive(Debug, Clone, DataSize)]
pub struct Content {
    /// The raw content stream parts. usually one, but could be any number.
    pub parts: Vec<Stream<()>>,
}

impl Content {
    pub fn operations(&self, resolve: &impl Resolve) -> Result<Vec<Op>> {
        let mut data = vec![];
        for part in self.parts.iter() {
            data.extend_from_slice(&t!(part.data(resolve)));
        }
        parse_ops(&data, resolve)
    }
}

pub fn parse_ops(data: &[u8], resolve: &impl Resolve) -> Result<Vec<Op>> {
    let mut ops = OpBuilder::new();
    ops.parse(data, resolve)?;
    Ok(ops.ops)
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
fn name(args: &mut impl Iterator<Item=Primitive>) -> Result<Name> {
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
fn rect(args: &mut impl Iterator<Item=Primitive>) -> Result<ViewRect> {
    let x = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let y = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let width = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    let height = args.next().ok_or(PdfError::NoOpArg)?.as_number()?;
    Ok(ViewRect { x, y, width, height })
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
fn array(args: &mut impl Iterator<Item=Primitive>) -> Result<Vec<Primitive>> {
    match args.next() {
        Some(Primitive::Array(arr)) => Ok(arr),
        None => Ok(vec![]),
        _ => Err(PdfError::NoOpArg)
    }
}

fn expand_abbr_name(name: SmallString, alt: &[(&str, &str)]) -> SmallString {
    for &(p, r) in alt {
        if name == p {
            return r.into();
        }
    }
    name
}
fn expand_abbr(p: Primitive, alt: &[(&str, &str)]) -> Primitive {
    match p {
        Primitive::Name(name) => Primitive::Name(expand_abbr_name(name, alt)),
        Primitive::Array(items) => Primitive::Array(items.into_iter().map(|p| expand_abbr(p, alt)).collect()),
        p => p
    }
}

fn inline_image(lexer: &mut Lexer, resolve: &impl Resolve) -> Result<Arc<ImageXObject>> {
    let mut dict = Dictionary::new();
    loop {
        let backup_pos = lexer.get_pos();
        let obj = parse_with_lexer(lexer, &NoResolve, ParseFlags::ANY);
        let key = match obj {
            Ok(Primitive::Name(key)) => key,
            Err(e) if e.is_eof() => return Err(e),
            Err(_) => {
                lexer.set_pos(backup_pos);
                break;
            }
            Ok(_) => bail!("invalid key type")
        };
        let key = expand_abbr_name(key, &[
            ("BPC", "BitsPerComponent"),
            ("CS", "ColorSpace"),
            ("D", "Decode"),
            ("DP", "DecodeParms"),
            ("F", "Filter"),
            ("H", "Height"),
            ("IM", "ImageMask"),
            ("I", "Interpolate"),
            ("W", "Width"),
        ]);
        let val = parse_with_lexer(lexer, &NoResolve, ParseFlags::ANY)?;
        dict.insert(key, val);
    }
    lexer.next_expect("ID")?;
    let data_start = lexer.get_pos() + 1;

    // find the end before try parsing.
    if lexer.seek_substr("\nEI").is_none() {
        bail!("inline image exceeds expected data range");
    }    
    let data_end = lexer.get_pos() - 3;

    // ugh
    let bits_per_component = dict.get("BitsPerComponent").map(|p| p.as_integer()).transpose()?;
    let color_space = dict.get("ColorSpace").map(|p| ColorSpace::from_primitive(expand_abbr(p.clone(), 
        &[
            ("G", "DeviceGray"),
            ("RGB", "DeviceRGB"),
            ("CMYK", "DeviceCMYK"),
            ("I", "Indexed")
        ]
    ), resolve)).transpose()?;
    let decode = dict.get("Decode").map(|p| Object::from_primitive(p.clone(), resolve)).transpose()?;
    let decode_parms = dict.get("DecodeParms").map(|p| p.clone().resolve(resolve)?.into_dictionary()).transpose()?.unwrap_or_default();
    let filter = dict.remove("Filter").map(|p| expand_abbr(p,
        &[
            ("AHx", "ASCIIHexDecode"),
            ("A85", "ASCII85Decode"),
            ("LZW", "LZWDecode"),
            ("Fl", "FlateDecode"),
            ("RL", "RunLengthDecode"),
            ("CCF", "CCITTFaxDecode"),
            ("DCT", "DCTDecode"),
        ]
    ));
    let filters = match filter {
        Some(Primitive::Array(parts)) => parts.into_iter()
            .map(|p| p.as_name().and_then(|kind| StreamFilter::from_kind_and_params(kind, decode_parms.clone(), resolve)))
            .collect::<Result<_>>()?,
        Some(Primitive::Name(kind)) => vec![StreamFilter::from_kind_and_params(&kind, decode_parms, resolve)?],
        None => vec![],
        _ => bail!("invalid filter")
    };
    
    let height = dict.require("InlineImage", "Height")?.as_u32()?;
    let image_mask = dict.get("ImageMask").map(|p| p.as_bool()).transpose()?.unwrap_or(false);
    let intent = dict.remove("Intent").map(|p| RenderingIntent::from_primitive(p, &NoResolve)).transpose()?;
    let interpolate = dict.get("Interpolate").map(|p| p.as_bool()).transpose()?.unwrap_or(false);
    let width = dict.require("InlineImage", "Width")?.as_u32()?;

    let image_dict = ImageDict {
        width,
        height,
        color_space,
        bits_per_component,
        intent,
        image_mask,
        mask: None,
        decode,
        interpolate,
        struct_parent: None,
        id: None,
        smask: None,
        other: dict,
    };

    let data = lexer.new_substr(data_start .. data_end).to_vec();

    Ok(Arc::new(ImageXObject { inner: Stream::from_compressed(image_dict, data, filters) }))
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
            let obj = parse_with_lexer(&mut lexer, resolve, ParseFlags::ANY);
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
                    let operator = t!(op.as_str(), op);
                    match self.add(operator, buffer.drain(..), &mut lexer, resolve) {
                        Ok(()) => {},
                        Err(e) if resolve.options().allow_invalid_ops => {
                            warn!("OP Err: {:?}", e);
                        },
                        Err(e) => return Err(e),
                    }
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
    fn add(&mut self, op: &str, mut args: impl Iterator<Item=Primitive>, lexer: &mut Lexer, resolve: &impl Resolve) -> Result<()> {
        use Winding::*;

        let ops = &mut self.ops;
        let mut push = move |op| ops.push(op);

        match op {
            "b"   => {
                push(Op::Close);
                push(Op::FillAndStroke { winding: NonZero });
            },
            "B"   => push(Op::FillAndStroke { winding: NonZero }),
            "b*"  => {
                push(Op::Close);
                push(Op::FillAndStroke { winding: EvenOdd });
            }
            "B*"  => push(Op::FillAndStroke { winding: EvenOdd }),
            "BDC" => push(Op::BeginMarkedContent {
                tag: name(&mut args)?,
                properties: Some(args.next().ok_or(PdfError::NoOpArg)?)
            }),
            "BI"  => push(Op::InlineImage { image: inline_image(lexer, resolve)? }),
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
            "Do" | "Do0" => {
                names!(args, name);
                push(Op::XObject { name });
            }
            "DP"  => push(Op::MarkedContentPoint {
                tag: name(&mut args)?,
                properties: Some(args.next().ok_or(PdfError::NoOpArg)?)
            }),
            "EI"  => bail!("Parse Error. Unexpected 'EI'"),
            "EMC" => push(Op::EndMarkedContent),
            "ET"  => push(Op::EndText),
            "EX"  => self.compability_section = false,
            "f" |
            "F"   => push(Op::Fill { winding: NonZero }),
            "f*"  => push(Op::Fill { winding: EvenOdd }),
            "G"   => push(Op::StrokeColor { color: Color::Gray(number(&mut args)?) }),
            "g"   => push(Op::FillColor { color: Color::Gray(number(&mut args)?) }),
            "gs"  => push(Op::GraphicsState { name: name(&mut args)? }),
            "h"   => push(Op::Close),
            "i"   => push(Op::Flatness { tolerance: number(&mut args)? }),
            "ID"  => bail!("Parse Error. Unexpected 'ID'"),
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
                let s = name(&mut args)?;
                let intent = RenderingIntent::from_str(&s)
                    .ok_or_else(|| PdfError::Other { msg: format!("invalid rendering intent {}", s) })?;
                push(Op::RenderingIntent { intent });
            },
            "s"   => {
                push(Op::Close);
                push(Op::Stroke);
            }
            "S"   => push(Op::Stroke),
            "SC" | "SCN" => {
                push(Op::StrokeColor { color: Color::Other(args.collect()) });
            }
            "sc" | "scn" => {
                push(Op::FillColor { color: Color::Other(args.collect()) });
            }
            "sh"  => {

            }
            "T*"  => push(Op::TextNewline),
            "Tc"  => push(Op::CharSpacing { char_space: number(&mut args)? }),
            "Td"  => push(Op::MoveTextPosition { translation: point(&mut args)? }),
            "TD"  => {
                let translation = point(&mut args)?;
                push(Op::Leading { leading: -translation.y });
                push(Op::MoveTextPosition { translation });
            }
            "Tf"  => push(Op::TextFont { name: name(&mut args)?, size: number(&mut args)? }),
            "Tj"  => push(Op::TextDraw { text: string(&mut args)? }),
            "TJ"  => {
                let mut result = Vec::<TextDrawAdjusted>::new();

                for spacing_or_text in array(&mut args)?.into_iter() {
                    let spacing_or_text = match spacing_or_text {
                        Primitive::Integer(i) => TextDrawAdjusted::Spacing(i as f32),
                        Primitive::Number(f) => TextDrawAdjusted::Spacing(f),
                        Primitive::String(text) => TextDrawAdjusted::Text(text),
                        p => bail!("invalid primitive in TJ operator: {:?}", p)
                    };

                    result.push(spacing_or_text);
                }

                push(Op::TextDrawAdjusted { array: result })
            }
            "TL"  => push(Op::Leading { leading: number(&mut args)? }),
            "Tm"  => push(Op::SetTextMatrix { matrix: matrix(&mut args)? }), 
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
                push(Op::TextDraw { text: string(&mut args)? });
            }
            "\""  => {
                push(Op::WordSpacing { word_space: number(&mut args)? });
                push(Op::CharSpacing { char_space: number(&mut args)? });
                push(Op::TextNewline);
                push(Op::TextDraw { text: string(&mut args)? });
            }
            o if !self.compability_section => {
                bail!("invalid operator {}", o)
            },
            _ => {}
        }
        Ok(())
    }
}

impl Object for Content {
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        type ContentStream = Stream<()>;
        let mut parts: Vec<ContentStream> = vec![];

        match p {
            Primitive::Array(arr) => {
                for p in arr {
                    let part = t!(ContentStream::from_primitive(p, resolve));
                    parts.push(part);
                }
            }
            Primitive::Reference(r) => return Self::from_primitive(t!(resolve.resolve(r)), resolve),
            p => {
                let part = t!(ContentStream::from_primitive(p, resolve));
                parts.push(part);
            }
        }

        Ok(Content { parts })
    }
}

#[derive(Debug, DataSize, DeepClone, Clone)]
pub struct FormXObject {
    pub stream: Stream<FormDict>,
}
impl FormXObject {
    pub fn dict(&self) -> &FormDict {
        &self.stream.info.info
    }
    pub fn operations(&self, resolve: &impl Resolve) -> Result<Vec<Op>> {
        let mut ops = OpBuilder::new();
        let data = self.stream.data(resolve)?;
        t!(ops.parse(&data, resolve));
        Ok(ops.ops)
    }
}
impl Object for FormXObject {
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let stream = t!(Stream::<FormDict>::from_primitive(p, resolve));
        Ok(FormXObject {
            stream,
        })
    }
}
impl ObjectWrite for FormXObject {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let mut stream = self.stream.to_pdf_stream(update)?;
        stream.info.insert("Subtype", Name::from("Form"));
        Ok(stream.into())
    }
}

#[allow(clippy::float_cmp)]  // TODO
pub fn serialize_ops(mut ops: &[Op]) -> Result<Vec<u8>> {
    use std::io::Write;

    let mut data = Vec::new();
    let mut current_point = None;
    let f = &mut data;

    while ops.len() > 0 {
        let mut advance = 1;
        match ops[0] {
            Op::BeginMarkedContent { ref tag, properties: Some(ref name) } => {
                serialize_name(tag, f)?;
                write!(f, " ")?;
                name.serialize(f)?;
                writeln!(f, " BDC")?;
            }
            Op::BeginMarkedContent { ref tag, properties: None } => {
                serialize_name(tag, f)?;
                writeln!(f, " BMC")?;
            }
            Op::MarkedContentPoint { ref tag, properties: Some(ref name) } => {
                serialize_name(tag, f)?;
                write!(f, " ")?;
                name.serialize(f)?;
                writeln!(f, " DP")?;
            }
            Op::MarkedContentPoint { ref tag, properties: None } => {
                serialize_name(tag, f)?;
                writeln!(f, " MP")?;
            }
            Op::EndMarkedContent => writeln!(f, "EMC")?,
            Op::Close => match ops.get(1) {
                Some(Op::Stroke) => {
                    writeln!(f, "s")?;
                    advance += 1;
                }
                Some(Op::FillAndStroke { winding: Winding::NonZero }) => {
                    writeln!(f, "b")?;
                    advance += 1;
                }
                Some(Op::FillAndStroke { winding: Winding::EvenOdd }) => {
                    writeln!(f, "b*")?;
                    advance += 1;
                }
                _ => writeln!(f, "h")?,
            }
            Op::MoveTo { p } => {
                writeln!(f, "{} m", p)?;
                current_point = Some(p);
            }
            Op::LineTo { p } => {
                writeln!(f, "{} l", p)?;
                current_point = Some(p);
            },
            Op::CurveTo { c1, c2, p } => {
                if Some(c1) == current_point {
                    writeln!(f, "{} {} v", c2, p)?;
                } else if c2 == p {
                    writeln!(f, "{} {} y", c1, p)?;
                } else {
                    writeln!(f, "{} {} {} c", c1, c2, p)?;
                }
                current_point = Some(p);
            },
            Op::Rect { rect } => writeln!(f, "{} re", rect)?,
            Op::EndPath => writeln!(f, "n")?,
            Op::Stroke => writeln!(f, "S")?,
            Op::FillAndStroke { winding: Winding::NonZero } => writeln!(f, "B")?,
            Op::FillAndStroke { winding: Winding::EvenOdd } => writeln!(f, "B*")?,
            Op::Fill { winding: Winding::NonZero } => writeln!(f, "f")?,
            Op::Fill { winding: Winding::EvenOdd } => writeln!(f, "f*")?,
            Op::Shade { ref name } => {
                serialize_name(name, f)?;
                writeln!(f, " sh")?;
            },
            Op::Clip { winding: Winding::NonZero } => writeln!(f, "W")?,
            Op::Clip { winding: Winding::EvenOdd } => writeln!(f, "W*")?,
            Op::Save => writeln!(f, "q")?,
            Op::Restore => writeln!(f, "Q")?,
            Op::Transform { matrix } => writeln!(f, "{} cm", matrix)?,
            Op::LineWidth { width } => writeln!(f, "{} w", width)?,
            Op::Dash { ref pattern, phase } => writeln!(f, "[{}] {} d", pattern.iter().format(" "), phase)?,
            Op::LineJoin { join } => writeln!(f, "{} j", join as u8)?,
            Op::LineCap { cap } => writeln!(f, "{} J", cap as u8)?,
            Op::MiterLimit { limit } => writeln!(f, "{} M", limit)?,
            Op::Flatness { tolerance } => writeln!(f, "{} i", tolerance)?,
            Op::GraphicsState { ref name } => {
                serialize_name(name, f)?;
                writeln!(f, " gs")?;
            },
            Op::StrokeColor { color: Color::Gray(g) } => writeln!(f, "{} G", g)?,
            Op::StrokeColor { color: Color::Rgb(rgb) } => writeln!(f, "{} RG", rgb)?,
            Op::StrokeColor { color: Color::Cmyk(cmyk) } => writeln!(f, "{} K", cmyk)?,
            Op::StrokeColor { color: Color::Other(ref args) } =>  {
                for p in args {
                    p.serialize(f)?;
                    write!(f, " ")?;
                }
                writeln!(f, "SCN")?;
            }
            Op::FillColor { color: Color::Gray(g) } => writeln!(f, "{} g", g)?,
            Op::FillColor { color: Color::Rgb(rgb) } => writeln!(f, "{} rg", rgb)?,
            Op::FillColor { color: Color::Cmyk(cmyk) } => writeln!(f, "{} k", cmyk)?,
            Op::FillColor { color: Color::Other(ref args) } => {
                for p in args {
                    p.serialize(f)?;
                    write!(f, " ")?;
                }
                writeln!(f, "scn")?;
            }
            Op::FillColorSpace { ref name } => {
                serialize_name(name, f)?;
                writeln!(f, " cs")?;
            },
            Op::StrokeColorSpace { ref name } => {
                serialize_name(name, f)?;
                writeln!(f, " CS")?;
            },

            Op::RenderingIntent { intent } => writeln!(f, "{} ri", intent.to_str())?,
            Op::BeginText => writeln!(f, "BT")?,
            Op::EndText => writeln!(f, "ET")?,
            Op::CharSpacing { char_space } => writeln!(f, "{} Tc", char_space)?,
            Op::WordSpacing { word_space } => {
                if let [
                    Op::CharSpacing { char_space },
                    Op::TextNewline,
                    Op::TextDraw { ref text },
                    ..
                ] = ops[1..] {
                    write!(f, "{} {} ", word_space, char_space)?;
                    text.serialize(f)?;
                    writeln!(f, " \"")?;
                    advance += 3;
                } else {
                    writeln!(f, "{} Tw", word_space)?;
                }
            }
            Op::TextScaling { horiz_scale } => writeln!(f, "{} Tz", horiz_scale)?,
            Op::Leading { leading } => match ops[1..] {
                [Op::MoveTextPosition { translation }, ..] if leading == -translation.x => {
                    writeln!(f, "{} {} TD", translation.x, translation.y)?;
                    advance += 1;
                }
                _ => {
                    writeln!(f, "{} TL", leading)?;
                }
            }
            Op::TextFont { ref name, ref size } => {
                serialize_name(name, f)?;
                writeln!(f, " {} Tf", size)?;
            },
            Op::TextRenderMode { mode } => writeln!(f, "{} Tr", mode as u8)?,
            Op::TextRise { rise } => writeln!(f, "{} Ts", rise)?,
            Op::MoveTextPosition { translation } => writeln!(f, "{} {} Td", translation.x, translation.y)?,
            Op::SetTextMatrix { matrix } => writeln!(f, "{} Tm", matrix)?,
            Op::TextNewline => {
                if let [Op::TextDraw { ref text }, ..] = ops[1..] {
                    text.serialize(f)?;
                    writeln!(f, " '")?;
                    advance += 1;
                } else {
                    writeln!(f, "T*")?;
                }
            },
            Op::TextDraw { ref text } => {
                text.serialize(f)?;
                writeln!(f, " Tj")?;
            },
            Op::TextDrawAdjusted { ref array } => {
                write!(f, "[")?;
                for (i, val) in array.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    match val {
                        TextDrawAdjusted::Spacing(s) => write!(f, "{s}")?,
                        TextDrawAdjusted::Text(data) => data.serialize(f)?,
                    }
                }
                writeln!(f, "] TJ")?;
            },
            Op::InlineImage { image: _ } => unimplemented!(),
            Op::XObject { ref name } => {
                serialize_name(name, f)?;
                writeln!(f, " Do")?;
            },
        }
        ops = &ops[advance..];
    }
    Ok(data)
}

impl Content {
    pub fn from_ops(operations: Vec<Op>) -> Self {
        let data = serialize_ops(&operations).unwrap();
        Content {
            parts: vec![Stream::new((), data)]
        }
    }
}

impl ObjectWrite for Content {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        if self.parts.len() == 1 {
            let obj = self.parts[0].to_primitive(update)?;
            match obj {
                Primitive::Reference(_) => Ok(obj),
                _ => update.create(obj)?.to_primitive(update)
            }
        } else {
            self.parts.to_primitive(update)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, DataSize)]
pub enum Winding {
    EvenOdd,
    NonZero
}

#[derive(Debug, Copy, Clone, PartialEq, DataSize)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

#[derive(Debug, Copy, Clone, PartialEq, DataSize)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

#[cfg(feature = "euclid")]
pub struct PdfSpace();

#[derive(Debug, Copy, Clone, PartialEq, Default, DataSize)]
#[repr(C, align(8))]
pub struct Point {
    pub x: f32,
    pub y: f32
}
impl Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.x, self.y)
    }
}
#[cfg(feature = "euclid")]
impl Into<euclid::Point2D<f32, PdfSpace>> for Point {
    fn into(self) -> euclid::Point2D<f32, PdfSpace> {
        let Point { x, y } = self;

        euclid::Point2D::new(x, y)
    }
}
#[cfg(feature = "euclid")]
impl From<euclid::Point2D<f32, PdfSpace>> for Point {
    fn from(from: euclid::Point2D<f32, PdfSpace>) -> Self {
        let euclid::Point2D { x, y, .. } = from;

        Point { x, y }
    }
}
#[cfg(feature = "euclid")]
impl Into<euclid::Vector2D<f32, PdfSpace>> for Point {
    fn into(self) -> euclid::Vector2D<f32, PdfSpace> {
        let Point { x, y } = self;

        euclid::Vector2D::new(x, y)
    }
}
#[cfg(feature = "euclid")]
impl From<euclid::Vector2D<f32, PdfSpace>> for Point {
    fn from(from: euclid::Vector2D<f32, PdfSpace>) -> Self {
        let euclid::Vector2D { x, y, .. } = from;

        Point { x, y }
    }
}

/// ISO 32000-2:2020(E) Table 58 Pg 186 - ViewRect
/// Path construction operators - {x y width height re}
/// Append a rectangle to the current path as a complete
/// subpath, with lower-left corner (x, y) and dimensions
/// width and height in user space.
#[derive(Debug, Copy, Clone, PartialEq, DataSize)]
#[repr(C, align(8))]
pub struct ViewRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[deprecated]
pub type Rect = ViewRect;

impl Display for ViewRect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} {} {}", self.x, self.y, self.width, self.height)
    }
}
#[cfg(feature = "euclid")]
impl Into<euclid::Box2D<f32, PdfSpace>> for ViewRect {
    fn into(self) -> euclid::Box2D<f32, PdfSpace> {
        let ViewRect { x, y, width, height } = self;

        assert!(width > 0.0);
        assert!(height > 0.0);

        euclid::Box2D::new(euclid::Point2D::new(x, y), euclid::Point2D::new(x + width, y + height))
    }
}
#[cfg(feature = "euclid")]
impl From<euclid::Box2D<f32, PdfSpace>> for ViewRect {
    fn from(from: euclid::Box2D<f32, PdfSpace>) -> Self {
        let euclid::Box2D { min: euclid::Point2D { x, y, .. }, max: euclid::Point2D { x: x2, y: y2, .. }, .. } = from;

        assert!(x < x2);
        assert!(y < y2);

        ViewRect {
            x, y, width: x2 - x, height: y2 - y
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, DataSize, DeepClone)]
#[repr(C, align(8))]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}
impl Display for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} {} {} {} {}", self.a, self.b, self.c, self.d, self.e, self.f)
    }
}
impl Default for Matrix {
    fn default() -> Self {
        Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
}
impl Object for Matrix {
    fn from_primitive(p: Primitive, _resolve: &impl Resolve) -> Result<Self> {
        matrix(&mut p.into_array()?.into_iter())
    }
}
impl ObjectWrite for Matrix {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let Matrix { a, b, c, d, e, f } = *self;
        Primitive::array::<f32, _, _, _>([a, b, c, d, e, f].iter(), update)
    }
}
#[cfg(feature = "euclid")]
impl Into<euclid::Transform2D<f32, PdfSpace, PdfSpace>> for Matrix {
    fn into(self) -> euclid::Transform2D<f32, PdfSpace, PdfSpace> {
        let Matrix { a, b, c, d, e, f} = self;

        euclid::Transform2D::new(a, b, c, d, e, f)
    }
}
#[cfg(feature = "euclid")]
impl From<euclid::Transform2D<f32, PdfSpace, PdfSpace>> for Matrix {
    fn from(from: euclid::Transform2D<f32, PdfSpace, PdfSpace>) -> Self {
        let euclid::Transform2D { m11: a, m12: b, m21: c, m22: d, m31: e, m32: f, .. } = from;

        Matrix {
            a, b, c, d, e, f
        }
    }
}

#[derive(Debug, Clone, DataSize)]
pub enum Color {
    Gray(f32),
    Rgb(Rgb),
    Cmyk(Cmyk),
    Other(Vec<Primitive>),
}

#[derive(Debug, Copy, Clone, PartialEq, DataSize)]
pub enum TextMode {
    Fill,
    Stroke,
    FillThenStroke,
    Invisible,
    FillAndClip,
    StrokeAndClip
}

#[derive(Debug, Copy, Clone, PartialEq, DataSize)]
pub struct Rgb {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}
impl Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} {}", self.red, self.green, self.blue)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, DataSize)]
pub struct Cmyk {
    pub cyan: f32,
    pub magenta: f32,
    pub yellow: f32,
    pub key: f32,
}
impl Display for Cmyk {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} {} {}", self.cyan, self.magenta, self.yellow, self.key)
    }
}

#[derive(Debug, Clone, DataSize)]
pub enum TextDrawAdjusted {
    Text(PdfString),
    Spacing(f32),
}

impl Display for TextDrawAdjusted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => write!(f, "{:?}", text),
            Self::Spacing(spacing) => spacing.fmt(f),
        }
    }
}

/// Graphics Operator
/// 
/// See PDF32000 A.2
#[derive(Debug, Clone, DataSize)]
pub enum Op {
    /// Begin a marked comtent sequence
    /// 
    /// Pairs with the following EndMarkedContent.
    /// 
    /// generated by operators `BMC` and `BDC`
    BeginMarkedContent { tag: Name, properties: Option<Primitive> },

    /// End a marked content sequence.
    /// 
    /// Pairs with the previous BeginMarkedContent.
    /// 
    /// generated by operator `EMC`
    EndMarkedContent,

    /// A marked content point.
    /// 
    /// generated by operators `MP` and `DP`.
    MarkedContentPoint { tag: Name, properties: Option<Primitive> },


    Close,
    MoveTo { p: Point },
    LineTo { p: Point },
    CurveTo { c1: Point, c2: Point, p: Point },
    Rect { rect: ViewRect },
    EndPath,

    Stroke,

    /// Fill and Stroke operation
    /// 
    /// generated by operators `b`, `B`, `b*`, `B*`
    /// `close` indicates whether the path should be closed first
    FillAndStroke { winding: Winding },


    Fill { winding: Winding },

    /// Fill using the named shading pattern
    /// 
    /// operator: `sh`
    Shade { name: Name },

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

    GraphicsState { name: Name },

    StrokeColor { color: Color },
    FillColor { color: Color },

    FillColorSpace { name: Name },
    StrokeColorSpace { name: Name },

    RenderingIntent { intent: RenderingIntent },

    BeginText,
    EndText,

    CharSpacing { char_space: f32 },
    WordSpacing { word_space: f32 },
    TextScaling { horiz_scale: f32 },
    Leading { leading: f32 },
    TextFont { name: Name, size: f32 },
    TextRenderMode { mode: TextMode },

    /// `Ts`
    TextRise { rise: f32 },

    /// `Td`, `TD`
    MoveTextPosition { translation: Point },

    /// `Tm`
    SetTextMatrix { matrix: Matrix },

    /// `T*`
    TextNewline,

    /// `Tj`
    TextDraw { text: PdfString },

    TextDrawAdjusted { array: Vec<TextDrawAdjusted> },

    XObject { name: Name },

    InlineImage { image: Arc<ImageXObject> },
}

pub fn deep_clone_op(op: &Op, cloner: &mut impl Cloner, old_resources: &Resources, resources: &mut Resources) -> Result<Op> {
    match *op {
        Op::GraphicsState { ref name } => {
            if !resources.graphics_states.contains_key(name) {
                if let Some(gs) = old_resources.graphics_states.get(name) {
                    resources.graphics_states.insert(name.clone(), gs.deep_clone(cloner)?);
                }
            }
            Ok(Op::GraphicsState { name: name.clone() })
        }
        Op::MarkedContentPoint { ref tag, ref properties } => {
            Ok(Op::MarkedContentPoint { tag: tag.clone(), properties: properties.deep_clone(cloner)? })
        }
        Op::BeginMarkedContent { ref tag, ref properties } => {
            Ok(Op::BeginMarkedContent { tag: tag.clone(), properties: properties.deep_clone(cloner)? })
        }
        Op::TextFont { ref name, size } => {
            if !resources.fonts.contains_key(name) {
                if let Some(f) = old_resources.fonts.get(name) {
                    resources.fonts.insert(name.clone(), f.deep_clone(cloner)?);
                }
            }
            Ok(Op::TextFont { name: name.clone(), size })
        }
        Op::XObject { ref name } => {
            if !resources.xobjects.contains_key(name) {
                if let Some(xo) = old_resources.xobjects.get(name) {
                    resources.xobjects.insert(name.clone(), xo.deep_clone(cloner)?);
                }
            }
            Ok(Op::XObject { name: name.clone() })
        }
        ref op => Ok(op.clone())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_image() {
        let data = br###"
/W 768
/H 150
/BPC 1
/IM true
/F [/A85 /Fl]
ID
Gb"0F_%"1&#XD6"#B1qiGGG^V6GZ#ZkijB5'RjB4S^5I61&$Ni:Xh=4S_9KYN;c9MUZPn/h,c]oCLUmg*Fo?0Hs0nQHp41KkO\Ls5+g0aoD*btT?l]lq0YAucfaoqHp4
1KkO\Ls5+g0aoD*btT?l^#mD&ORf[0~>
EI
"###;
        let mut lexer = Lexer::new(data);
        assert!(inline_image(&mut lexer, &NoResolve).is_ok()); 
    }
}
