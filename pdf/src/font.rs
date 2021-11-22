use crate as pdf;
use crate::object::*;
use crate::primitive::*;
use crate::error::*;
use crate::encoding::Encoding;
use std::collections::HashMap;
use crate::parser::{Lexer, parse_with_lexer};
use utf16_ext::Utf16ReadExt;
use byteorder::BE;
use std::convert::TryInto;

#[allow(non_upper_case_globals, dead_code)] 
mod flags {
    pub const FixedPitch: u32    = 1 << 0;
    pub const Serif: u32         = 1 << 1;
    pub const Symbolic: u32      = 1 << 2;
    pub const Script: u32        = 1 << 3;
    pub const Nonsymbolic: u32   = 1 << 5;
    pub const Italic: u32        = 1 << 6;
    pub const AllCap: u32        = 1 << 16;
    pub const SmallCap: u32      = 1 << 17;
    pub const ForceBold: u32     = 1 << 18;
}

#[derive(Object, Debug, Copy, Clone)]
pub enum FontType {
    Type0,
    Type1,
    MMType1,
    Type3,
    TrueType,
    CIDFontType0, //Type1
    CIDFontType2, // TrueType
}

#[derive(Debug)]
pub struct Font {
    pub subtype: FontType,
    pub name: Option<String>,
    pub data: Result<FontData>,
    
    encoding: Option<Encoding>,
    
    to_unicode: Option<Stream>,
    
    /// other keys not mapped in other places. May change over time without notice, and adding things probably will break things. So don't expect this to be part of the stable API
    pub _other: Dictionary
}

#[derive(Debug)]
pub enum FontData {
    Type1(TFont),
    Type0(Type0Font),
    TrueType(TFont),
    CIDFontType0(CIDFont),
    CIDFontType2(CIDFont, Option<Vec<u16>>),
    Other(Dictionary),
    None,
}

impl Object for Font {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = p.into_dictionary(resolve)?;

        let subtype = FontType::from_primitive(dict.require("Font", "Subtype")?, resolve)?;

        // BaseFont is required for all FontTypes except Type3
        dict.expect("Font", "Type", "Font", true)?;
        let base_font_primitive = dict.get("BaseFont");
        let base_font = match (base_font_primitive, subtype) {
            (Some(name), _) => Some(name.clone().into_name()?),
            (None, FontType::Type3) => None,
            (_, _) => return Err(PdfError::MissingEntry {
                typ: "Font",
                field: "BaseFont".to_string()
            })
        };
        
        let encoding = dict.remove("Encoding").map(|p| Object::from_primitive(p, resolve)).transpose()?;

        let to_unicode = match dict.remove("ToUnicode") {
            Some(p) => Some(Stream::from_primitive(p, resolve)?),
            None => None
        };
        let _other = dict.clone();
        let data = { || 
            Ok(match subtype {
                FontType::Type0 => FontData::Type0(Type0Font::from_dict(dict, resolve)?),
                FontType::Type1 => FontData::Type1(TFont::from_dict(dict, resolve)?),
                FontType::TrueType => FontData::TrueType(TFont::from_dict(dict, resolve)?),
                FontType::CIDFontType0 => FontData::CIDFontType0(CIDFont::from_dict(dict, resolve)?),
                FontType::CIDFontType2 => {
                    let cid_map = match dict.remove("CIDToGIDMap") {
                        Some(p @ Primitive::Stream(_)) | Some(p @ Primitive::Reference(_)) => {
                            let stream: Stream<()> = Stream::from_primitive(p, resolve)?;
                            let data = stream.data()?;
                            Some(data.chunks(2).map(|c| (c[0] as u16) << 8 | c[1] as u16).collect())
                        },
                        _ => None
                    };
                    let cid_font = CIDFont::from_dict(dict, resolve)?;
                    FontData::CIDFontType2(cid_font, cid_map)
                }
                _ => FontData::Other(dict)
            })
        }();
        
        Ok(Font {
            subtype,
            name: base_font,
            data,
            encoding,
            to_unicode,
            _other
        })
    }
}

#[derive(Debug)]
pub struct Widths {
    values: Vec<f32>,
    default: f32,
    first_char: usize
}
impl Widths {
    pub fn get(&self, cid: usize) -> f32 {
        if cid < self.first_char {
            self.default
        } else {
            self.values.get(cid - self.first_char).cloned().unwrap_or(self.default)
        }
    }
    fn new(default: f32) -> Widths {
        Widths {
            default,
            values: Vec::new(),
            first_char: 0
        }
    }
    fn ensure_cid(&mut self, cid: usize) {
        if cid - self.first_char > self.values.capacity() {
            let missing = cid - self.values.len();
            self.values.reserve(missing);
        }
    }
    #[allow(clippy::float_cmp)]  // TODO
    fn set(&mut self, cid: usize, width: f32) {
        self._set(cid, width);
        debug_assert_eq!(self.get(cid), width);
    }
    fn _set(&mut self, cid: usize, width: f32) {
        use std::iter::repeat;

        if self.values.is_empty() {
            self.first_char = cid;
            self.values.push(width);
            return;
        }

        if cid == self.first_char + self.values.len() {
            self.values.push(width);
            return;
        }

        if cid < self.first_char {
            self.values.splice(0 .. 0, repeat(self.default).take(self.first_char - cid));
            self.first_char = cid;
            self.values[0] = width;
            return;
        }

        if cid > self.values.len() + self.first_char {
            self.ensure_cid(cid);
            self.values.extend(repeat(self.default).take(cid - self.first_char - self.values.len()));
            self.values.push(width);
            return;
        }

        self.values[cid - self.first_char] = width;
    }
}
impl Font {
    pub fn embedded_data(&self) -> Option<Result<&[u8]>> {
        match self.data.as_ref().ok()? {
            FontData::Type0(ref t) => t.descendant_fonts.get(0).and_then(|f| f.embedded_data()),
            FontData::CIDFontType0(ref c) | FontData::CIDFontType2(ref c, _) => c.font_descriptor.data(),
            FontData::Type1(ref t) | FontData::TrueType(ref t) => t.font_descriptor.as_ref().and_then(|d| d.data()),
            _ => None
        }
    }
    pub fn is_cid(&self) -> bool {
        matches!(self.data, Ok(FontData::CIDFontType0(_)) | Ok(FontData::CIDFontType2(_, _)))
    }
    pub fn cid_to_gid_map(&self) -> Option<&[u16]> {
        match self.data.as_ref().ok()? {
            FontData::Type0(ref inner) => inner.descendant_fonts.get(0).and_then(|f| f.cid_to_gid_map()),
            FontData::CIDFontType2(_, ref data) => data.as_ref().map(|v| &**v),
            _ => None
        }
    }
    pub fn encoding(&self) -> Option<&Encoding> {
        self.encoding.as_ref()
    }
    pub fn info(&self) -> Option<&TFont> {
        match self.data.as_ref().ok()? {
            FontData::Type1(ref info) => Some(info),
            FontData::TrueType(ref info) => Some(info),
            _ => None
        }
    }
    pub fn widths(&self, resolve: &impl Resolve) -> Result<Option<Widths>> {
        match self.data {
            Ok(FontData::Type0(ref t0)) => t0.descendant_fonts[0].widths(resolve),
            Ok(FontData::Type1(ref info)) | Ok(FontData::TrueType(ref info)) => {
                match *info {
                    TFont { first_char: Some(first), ref widths, .. } => Ok(Some(Widths {
                        default: 0.0,
                        first_char: first as usize,
                        values: widths.clone()
                    })),
                    _ => Ok(None)
                }
            },
            Ok(FontData::CIDFontType0(ref cid)) | Ok(FontData::CIDFontType2(ref cid, _)) => {
                let mut widths = Widths::new(cid.default_width);
                let mut iter = cid.widths.iter();
                while let Some(p) = iter.next() {
                    let c1 = p.as_integer()? as usize;
                    match iter.next() {
                        Some(&Primitive::Array(ref array)) => {
                            widths.ensure_cid(c1 + array.len() - 1);
                            for (i, w) in array.iter().enumerate() {
                                widths.set(c1 + i, w.as_number()?);
                            }
                        },
                        Some(&Primitive::Reference(r)) => {
                            match resolve.resolve(r)? {
                                Primitive::Array(array) => {
                                    widths.ensure_cid(c1 + array.len() - 1);
                                    for (i, w) in array.iter().enumerate() {
                                        widths.set(c1 + i, w.as_number()?);
                                    }
                                }
                                p => return Err(PdfError::Other { msg: format!("unexpected primitive in W array: {:?}", p) })
                            }
                        }
                        Some(&Primitive::Integer(c2)) => {
                            let w = try_opt!(iter.next()).as_number()?;
                            for c in (c1 as usize) ..= (c2 as usize) {
                                widths.set(c, w);
                            }
                        },
                        p => return Err(PdfError::Other { msg: format!("unexpected primitive in W array: {:?}", p) })
                    }
                }
                Ok(Some(widths))
            },
            _ => Ok(None)
        }
    }
    pub fn to_unicode(&self) -> Option<Result<ToUnicodeMap>> {
        self.to_unicode.as_ref().map(|s| s.data().and_then(parse_cmap))
    }
}
#[derive(Object, Debug)]
pub struct TFont {
    #[pdf(key="BaseFont")]
    pub base_font: Option<String>,
    
    /// per spec required, but some files lack it.
    #[pdf(key="FirstChar")]
    pub first_char: Option<i32>,
    
    /// same
    #[pdf(key="LastChar")]
    pub last_char: Option<i32>,
    
    #[pdf(key="Widths")]
    pub widths: Vec<f32>,
    
    #[pdf(key="FontDescriptor")]
    pub font_descriptor: Option<FontDescriptor>
}

#[derive(Object, Debug)]
pub struct Type0Font {
    #[pdf(key="DescendantFonts")]
    descendant_fonts: Vec<RcRef<Font>>,
    
    #[pdf(key="ToUnicode")]
    to_unicode: Option<Stream>,
}

#[derive(Object, Debug)]
pub struct CIDFont {
    #[pdf(key="CIDSystemInfo")]
    system_info: Dictionary,
    
    #[pdf(key="FontDescriptor")]
    font_descriptor: FontDescriptor,
    
    #[pdf(key="DW", default="1000.")]
    default_width: f32,
    
    #[pdf(key="W")]
    pub widths: Vec<Primitive>,

    #[pdf(other)]
    _other: Dictionary
}


#[derive(Object, Debug)]
pub struct FontDescriptor {
    #[pdf(key="FontName")]
    pub font_name: String,
    
    #[pdf(key="FontFamily")]
    pub font_family: Option<PdfString>,
    
    #[pdf(key="FontStretch")]
    pub font_stretch: Option<FontStretch>,

    #[pdf(key="FontWeight")]
    pub font_weight: Option<f32>,
    
    #[pdf(key="Flags")]
    pub flags: u32,
    
    #[pdf(key="FontBBox")]
    pub font_bbox: Rect,
    
    #[pdf(key="ItalicAngle")]
    pub italic_angle: f32,
    
    // required as per spec, but still missing in some cases
    #[pdf(key="Ascent")]
    pub ascent: Option<f32>,
    
    #[pdf(key="Descent")]
    pub descent: Option<f32>,
    
    #[pdf(key="Leading", default="0.")]
    pub leading: f32,
    
    #[pdf(key="CapHeight")]
    pub cap_height: Option<f32>,
    
    #[pdf(key="XHeight", default="0.")]
    pub xheight: f32,
    
    #[pdf(key="StemV", default="0.")]
    pub stem_v: f32,
    
    #[pdf(key="StemH", default="0.")]
    pub stem_h: f32,
    
    #[pdf(key="AvgWidth", default="0.")]
    pub avg_width: f32,
    
    #[pdf(key="MaxWidth", default="0.")]
    pub max_width: f32,
    
    #[pdf(key="MissingWidth", default="0.")]
    pub missing_width: f32,
    
    #[pdf(key="FontFile")]
    pub font_file: Option<Stream>,
    
    #[pdf(key="FontFile2")]
    pub font_file2: Option<Stream>,
    
    #[pdf(key="FontFile3")]
    pub font_file3: Option<Stream<FontStream3>>,
    
    #[pdf(key="CharSet")]
    pub char_set: Option<PdfString>
}
impl FontDescriptor {
    pub fn data(&self) -> Option<Result<&[u8]>> {
        if let Some(ref s) = self.font_file {
            Some(s.data())
        } else if let Some(ref s) = self.font_file2 {
            Some(s.data())
        } else if let Some(ref s) = self.font_file3 {
            Some(s.data())
        } else {
            None
        }
    }
}

#[derive(Object, Debug, Clone)]
#[pdf(key="Subtype")]
pub enum FontTypeExt {
    Type1C,
    CIDFontType0C,
    OpenType
}
#[derive(Object, Debug, Clone)]
pub struct FontStream3 {
    #[pdf(key="Subtype")]
    pub subtype: FontTypeExt
}

#[derive(Object, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded
}

#[derive(Clone, Debug)]
pub struct ToUnicodeMap {
    // todo: reduce allocations
    inner: HashMap<u16, String>
}
impl ToUnicodeMap {
    /// Create a new ToUnicodeMap from key/value pairs.
    /// 
    /// subject to change
    pub fn create(iter: impl Iterator<Item=(u16, String)>) -> Self {
        ToUnicodeMap { inner: iter.collect() }
    }
    pub fn get(&self, gid: u16) -> Option<&str> {
        self.inner.get(&gid).map(|s| s.as_str())
    }
}

fn utf16be_to_string(mut data: &[u8]) -> Result<String> {
    (&mut data)
        .utf16_chars::<BE>()
        .map(|r| r.map_err(|_| PdfError::Utf16Decode))
        .collect()
}
fn parse_cid(s: &PdfString) -> Result<u16> {
    let b = s.as_bytes();
    match b.len() {
        2 => Ok(u16::from_be_bytes(b.try_into().unwrap())),
        1 => Ok(b[0] as u16),
        _ => Err(PdfError::CidDecode),
    }
}
fn parse_cmap(data: &[u8]) -> Result<ToUnicodeMap> {
    let mut lexer = Lexer::new(data);
    let mut map = HashMap::new();
    while let Ok(substr) = lexer.next() {
        match substr.as_slice() {
            b"beginbfchar" => loop {
                let a = parse_with_lexer(&mut lexer, &NoResolve);
                let b = parse_with_lexer(&mut lexer, &NoResolve);
                match (a, b) {
                    (Ok(Primitive::String(cid_data)), Ok(Primitive::String(unicode_data))) => {
                        let cid = parse_cid(&cid_data)?;
                        let unicode = utf16be_to_string(unicode_data.as_bytes())?;
                        map.insert(cid, unicode);
                    }
                    _ => break,
                }
            },
            b"beginbfrange" => loop {
                let a = parse_with_lexer(&mut lexer, &NoResolve);
                let b = parse_with_lexer(&mut lexer, &NoResolve);
                let c = parse_with_lexer(&mut lexer, &NoResolve);
                match (a, b, c) {
                    (
                        Ok(Primitive::String(cid_start_data)),
                        Ok(Primitive::String(cid_end_data)),
                        Ok(Primitive::String(unicode_data)),
                    ) if unicode_data.data.len() > 0 => {
                        let cid_start = parse_cid(&cid_start_data)?;
                        let cid_end = parse_cid(&cid_end_data)?;
                        let mut unicode_data = unicode_data.into_bytes();

                        for cid in cid_start..=cid_end {
                            let unicode = utf16be_to_string(&unicode_data)?;
                            map.insert(cid, unicode);
                            *unicode_data.last_mut().unwrap() += 1;
                        }
                    }
                    (
                        Ok(Primitive::String(cid_start_data)),
                        Ok(Primitive::String(cid_end_data)),
                        Ok(Primitive::Array(unicode_data_arr)),
                    ) => {
                        let cid_start = parse_cid(&cid_start_data)?;
                        let cid_end = parse_cid(&cid_end_data)?;

                        for (cid, unicode_data) in (cid_start..=cid_end).zip(unicode_data_arr) {
                            let unicode =
                                utf16be_to_string(unicode_data.as_string()?.as_bytes())?;
                            map.insert(cid, unicode);
                        }
                    }
                    _ => break,
                }
            },
            b"endcmap" => break,
            _ => {}
        }
    }

    Ok(ToUnicodeMap { inner: map })
}
