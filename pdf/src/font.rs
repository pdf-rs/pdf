use crate as pdf;
use crate::object::*;
use crate::primitive::*;
use crate::error::*;
use crate::encoding::Encoding;
use std::collections::HashMap;
use crate::parser::{Lexer, parse_with_lexer, ParseFlags};
use std::convert::TryInto;
use std::sync::Arc;
use istring::SmallString;
use datasize::DataSize;

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

#[derive(Object, Debug, Copy, Clone, DataSize)]
pub enum FontType {
    Type0,
    Type1,
    MMType1,
    Type3,
    TrueType,
    CIDFontType0, //Type1
    CIDFontType2, // TrueType
}

#[derive(Debug, DataSize)]
pub struct Font {
    pub subtype: FontType,
    pub name: Option<Name>,
    pub data: FontData,

    encoding: Option<Encoding>,

    to_unicode: Option<Stream<()>>,

    /// other keys not mapped in other places. May change over time without notice, and adding things probably will break things. So don't expect this to be part of the stable API
    pub _other: Dictionary
}

#[derive(Debug, DataSize)]
pub enum FontData {
    Type1(TFont),
    Type0(Type0Font),
    TrueType(TFont),
    CIDFontType0(CIDFont),
    CIDFontType2(CIDFont),
    Other(Dictionary),
    None,
}

#[derive(Debug, DataSize)]
pub enum CidToGidMap {
    Identity,
    Table(Vec<u16>)
}
impl Object for CidToGidMap {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Name(name) if name == "Identity" => {
                Ok(CidToGidMap::Identity)
            }
            p @ Primitive::Stream(_) | p @ Primitive::Reference(_) => {
                let stream: Stream<()> = Stream::from_primitive(p, resolve)?;
                let data = stream.data(resolve)?;
                Ok(CidToGidMap::Table(data.chunks(2).map(|c| (c[0] as u16) << 8 | c[1] as u16).collect()))
            },
            p => Err(PdfError::UnexpectedPrimitive {
                expected: "/Identity or Stream",
                found: p.get_debug_name()
            })
        }
    }
}

impl Object for Font {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = p.resolve(resolve)?.into_dictionary()?;

        let subtype = t!(FontType::from_primitive(dict.require("Font", "Subtype")?, resolve));

        // BaseFont is required for all FontTypes except Type3
        dict.expect("Font", "Type", "Font", true)?;
        let base_font_primitive = dict.get("BaseFont");
        let base_font = match (base_font_primitive, subtype) {
            (Some(name), _) => Some(t!(t!(name.clone().resolve(resolve)).into_name(), name)),
            (None, FontType::Type3) => None,
            (_, _) => return Err(PdfError::MissingEntry {
                typ: "Font",
                field: "BaseFont".to_string()
            })
        };

        let encoding = dict.remove("Encoding").map(|p| Object::from_primitive(p, resolve)).transpose()?;

        let to_unicode = match dict.remove("ToUnicode") {
            Some(p) => Some(Stream::<()>::from_primitive(p, resolve)?),
            None => None
        };
        let _other = dict.clone();
        let data = match subtype {
            FontType::Type0 => FontData::Type0(Type0Font::from_dict(dict, resolve)?),
            FontType::Type1 => FontData::Type1(TFont::from_dict(dict, resolve)?),
            FontType::TrueType => FontData::TrueType(TFont::from_dict(dict, resolve)?),
            FontType::CIDFontType0 => FontData::CIDFontType0(CIDFont::from_dict(dict, resolve)?),
            FontType::CIDFontType2 => FontData::CIDFontType2(CIDFont::from_dict(dict, resolve)?),
            _ => FontData::Other(dict)
        };

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
impl ObjectWrite for Font {
    fn to_primitive(&self, _update: &mut impl Updater) -> Result<Primitive> {
        unimplemented!()
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
        if let Some(offset) = cid.checked_sub(self.first_char) { // cid may be < first_char
            // reserve difference of offset to capacity
            // if enough capacity to cover offset, saturates to zero, and reserve will do nothing
            self.values.reserve(offset.saturating_sub(self.values.capacity()));
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
    pub fn embedded_data(&self, resolve: &impl Resolve) -> Option<Result<Arc<[u8]>>> {
        match self.data {
            FontData::Type0(ref t) => t.descendant_fonts.get(0).and_then(|f| f.embedded_data(resolve)),
            FontData::CIDFontType0(ref c) | FontData::CIDFontType2(ref c) => c.font_descriptor.data(resolve),
            FontData::Type1(ref t) | FontData::TrueType(ref t) => t.font_descriptor.as_ref().and_then(|d| d.data(resolve)),
            _ => None
        }
    }
    pub fn is_cid(&self) -> bool {
        matches!(self.data, FontData::Type0(_) | FontData::CIDFontType0(_) | FontData::CIDFontType2(_))
    }
    pub fn cid_to_gid_map(&self) -> Option<&CidToGidMap> {
        match self.data {
            FontData::Type0(ref inner) => inner.descendant_fonts.get(0).and_then(|f| f.cid_to_gid_map()),
            FontData::CIDFontType0(ref f) | FontData::CIDFontType2(ref f) => f.cid_to_gid_map.as_ref(),
            _ => None
        }
    }
    pub fn encoding(&self) -> Option<&Encoding> {
        self.encoding.as_ref()
    }
    pub fn info(&self) -> Option<&TFont> {
        match self.data {
            FontData::Type1(ref info) => Some(info),
            FontData::TrueType(ref info) => Some(info),
            _ => None
        }
    }
    pub fn widths(&self, resolve: &impl Resolve) -> Result<Option<Widths>> {
        match self.data {
            FontData::Type0(ref t0) => t0.descendant_fonts[0].widths(resolve),
            FontData::Type1(ref info) | FontData::TrueType(ref info) => {
                match *info {
                    TFont { first_char: Some(first), ref widths, .. } => Ok(Some(Widths {
                        default: 0.0,
                        first_char: first as usize,
                        values: widths.clone()
                    })),
                    _ => Ok(None)
                }
            },
            FontData::CIDFontType0(ref cid) | FontData::CIDFontType2(ref cid) => {
                let mut widths = Widths::new(cid.default_width);
                let mut iter = cid.widths.iter();
                while let Some(p) = iter.next() {
                    let c1 = p.as_usize()?;
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
    pub fn to_unicode(&self, resolve: &impl Resolve) -> Option<Result<ToUnicodeMap>> {
        self.to_unicode.as_ref().map(|s| s.data(resolve).and_then(|d| parse_cmap(&d)))
    }
}
#[derive(Object, Debug, DataSize)]
pub struct TFont {
    #[pdf(key="BaseFont")]
    pub base_font: Option<Name>,

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

#[derive(Object, Debug, DataSize)]
pub struct Type0Font {
    #[pdf(key="DescendantFonts")]
    descendant_fonts: Vec<MaybeRef<Font>>,

    #[pdf(key="ToUnicode")]
    to_unicode: Option<Stream<()>>,
}

#[derive(Object, Debug, DataSize)]
pub struct CIDFont {
    #[pdf(key="CIDSystemInfo")]
    system_info: Dictionary,

    #[pdf(key="FontDescriptor")]
    font_descriptor: FontDescriptor,

    #[pdf(key="DW", default="1000.")]
    default_width: f32,

    #[pdf(key="W")]
    pub widths: Vec<Primitive>,

    #[pdf(key="CIDToGIDMap")]
    pub cid_to_gid_map: Option<CidToGidMap>,

    #[pdf(other)]
    _other: Dictionary
}


#[derive(Object, Debug, DataSize)]
pub struct FontDescriptor {
    #[pdf(key="FontName")]
    pub font_name: Name,

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
    pub font_file: Option<Stream<()>>,

    #[pdf(key="FontFile2")]
    pub font_file2: Option<Stream<()>>,

    #[pdf(key="FontFile3")]
    pub font_file3: Option<Stream<FontStream3>>,

    #[pdf(key="CharSet")]
    pub char_set: Option<PdfString>
}
impl FontDescriptor {
    pub fn data(&self, resolve: &impl Resolve) -> Option<Result<Arc<[u8]>>> {
        if let Some(ref s) = self.font_file {
            Some(s.data(resolve))
        } else if let Some(ref s) = self.font_file2 {
            Some(s.data(resolve))
        } else if let Some(ref s) = self.font_file3 {
            Some(s.data(resolve))
        } else {
            None
        }
    }
}

#[derive(Object, Debug, Clone, DataSize)]
#[pdf(key="Subtype")]
pub enum FontTypeExt {
    Type1C,
    CIDFontType0C,
    OpenType
}
#[derive(Object, Debug, Clone, DataSize)]
pub struct FontStream3 {
    #[pdf(key="Subtype")]
    pub subtype: FontTypeExt
}

#[derive(Object, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, DataSize)]
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
    inner: HashMap<u16, SmallString>
}
impl ToUnicodeMap {
    pub fn new() -> Self {
        ToUnicodeMap {
            inner: HashMap::new()
        }
    }
    /// Create a new ToUnicodeMap from key/value pairs.
    ///
    /// subject to change
    pub fn create(iter: impl Iterator<Item=(u16, SmallString)>) -> Self {
        ToUnicodeMap { inner: iter.collect() }
    }
    pub fn get(&self, gid: u16) -> Option<&str> {
        self.inner.get(&gid).map(|s| s.as_str())
    }
    pub fn insert(&mut self, gid: u16, unicode: SmallString) {
        self.inner.insert(gid, unicode);
    }
    pub fn iter(&self) -> impl Iterator<Item=(u16, &str)> {
        self.inner.iter().map(|(&gid, unicode)| (gid, unicode.as_str()))
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

/// helper function to decode UTF-16-BE data
/// takes a slice of u8 and returns an iterator for char or an decoding error
pub fn utf16be_to_char(
    data: &[u8],
) -> impl Iterator<Item = std::result::Result<char, std::char::DecodeUtf16Error>> + '_ {
    char::decode_utf16(data.chunks(2).map(|w| u16::from_be_bytes([w[0], w[1]])))
}
/// converts UTF16-BE to a string replacing illegal/unknown characters
pub fn utf16be_to_string_lossy(data: &[u8]) -> String {
    utf16be_to_char(data)
        .map(|r| r.unwrap_or(std::char::REPLACEMENT_CHARACTER))
        .collect()
}
/// converts UTF16-BE to a string errors out in illegal/unknonw characters
pub fn utf16be_to_string(data: &[u8]) -> pdf::error::Result<SmallString> {
    utf16be_to_char(data)
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
    let mut map = ToUnicodeMap::new();
    while let Ok(substr) = lexer.next() {
        match substr.as_slice() {
            b"beginbfchar" => loop {
                let a = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::STRING);
                if let Err(_) = a {
                    break;
                }
                let b = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::STRING);
                match (a, b) {
                    (Ok(Primitive::String(cid_data)), Ok(Primitive::String(unicode_data))) => {
                        let cid = parse_cid(&cid_data)?;
                        let bytes = unicode_data.as_bytes();
                        match utf16be_to_string(bytes) {
                            Ok(unicode) => map.insert(cid, unicode),
                            Err(_) => warn!("invalid unicode for cid {cid} {bytes:?}"),
                        }
                    }
                    _ => break,
                }
            },
            b"beginbfrange" => loop {
                let a = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::STRING);
                if let Err(_) = a {
                    break;
                }
                let b = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::STRING);
                let c = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::STRING | ParseFlags::ARRAY);
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
                            match utf16be_to_string(&unicode_data) {
                                Ok(unicode) => map.insert(cid, unicode),
                                Err(_) => warn!("invalid unicode for cid {cid} {unicode_data:?}"),
                            }
                            let last = unicode_data.last_mut().unwrap();
                            if *last < 255 {
                                *last += 1;
                            } else {
                                break;
                            }
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
                            let bytes = unicode_data.as_string()?.as_bytes();
                            match utf16be_to_string(bytes) {
                                Ok(unicode) => map.insert(cid, unicode),
                                Err(_) => warn!("invalid unicode for cid {cid} {bytes:?}"),
                            }
                        }
                    }
                    _ => break,
                }
            },
            b"endcmap" => break,
            _ => {}
        }
    }

    Ok(map)
}

#[cfg(test)]
mod tests {

    use crate::font::{utf16be_to_string, utf16be_to_char, utf16be_to_string_lossy};
    #[test]
    fn utf16be_to_string_quick() {
        let v = vec![0x20, 0x09];
        let s = utf16be_to_string(&v);
        assert_eq!(s.unwrap(), "\u{2009}");
        assert!(!v.is_empty());
    }

    #[test]
    fn test_to_char() {
        // 𝄞mus<invalid>ic<invalid>
        let v = [
            0xD8, 0x34, 0xDD, 0x1E, 0x00, 0x6d, 0x00, 0x75, 0x00, 0x73, 0xDD, 0x1E, 0x00, 0x69, 0x00,
            0x63, 0xD8, 0x34,
        ];

        assert_eq!(
            utf16be_to_char(&v)
                .map(|r| r.map_err(|e| e.unpaired_surrogate()))
                .collect::<Vec<_>>(),
            vec![
                Ok('𝄞'),
                Ok('m'),
                Ok('u'),
                Ok('s'),
                Err(0xDD1E),
                Ok('i'),
                Ok('c'),
                Err(0xD834)
            ]
        );

        let mut lossy = String::from("𝄞mus");
        lossy.push(std::char::REPLACEMENT_CHARACTER);
        lossy.push('i');
        lossy.push('c');
        lossy.push(std::char::REPLACEMENT_CHARACTER);

        let r = utf16be_to_string(&v);
        if let Err(r) = r {
            // FIXME: compare against PdfError::Utf16Decode variant
            assert_eq!(r.to_string(), "UTF16 decode error");
        }
        assert_eq!(utf16be_to_string(&v[..8]).unwrap(), String::from("𝄞mu"));
        assert_eq!(utf16be_to_string_lossy(&v), lossy);
    }
}
