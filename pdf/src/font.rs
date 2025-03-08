use crate as pdf;
use crate::encoding::Encoding;
use crate::error::*;
use crate::object::*;
use crate::parser::{parse_with_lexer, Lexer, ParseFlags};
use crate::primitive::*;
use datasize::DataSize;
use istring::SmallString;
use itertools::Itertools;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::Write;
use std::sync::Arc;

#[allow(non_upper_case_globals, dead_code)]
mod flags {
    pub const FixedPitch: u32 = 1 << 0;
    pub const Serif: u32 = 1 << 1;
    pub const Symbolic: u32 = 1 << 2;
    pub const Script: u32 = 1 << 3;
    pub const Nonsymbolic: u32 = 1 << 5;
    pub const Italic: u32 = 1 << 6;
    pub const AllCap: u32 = 1 << 16;
    pub const SmallCap: u32 = 1 << 17;
    pub const ForceBold: u32 = 1 << 18;
}

#[derive(Object, ObjectWrite, Debug, Copy, Clone, DataSize, DeepClone)]
pub enum FontType {
    Type0,
    Type1,
    MMType1,
    Type3,
    TrueType,
    CIDFontType0, //Type1
    CIDFontType2, // TrueType
}

#[derive(Debug, DataSize, DeepClone)]
pub struct Font {
    pub subtype: FontType,
    pub name: Option<Name>,
    pub data: FontData,

    pub encoding: Option<Encoding>,

    // FIXME: Should use RcRef<Stream>
    pub to_unicode: Option<RcRef<Stream<()>>>,

    /// other keys not mapped in other places. May change over time without notice, and adding things probably will break things. So don't expect this to be part of the stable API
    pub _other: Dictionary,
}

#[derive(Debug, DataSize, DeepClone)]
pub enum FontData {
    Type1(TFont),
    Type0(Type0Font),
    TrueType(TFont),
    CIDFontType0(CIDFont),
    CIDFontType2(CIDFont),
    Other(Dictionary),
}

#[derive(Debug, DataSize, DeepClone)]
pub enum CidToGidMap {
    Identity,
    Table(Vec<u16>),
}
impl Object for CidToGidMap {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Name(name) if name == "Identity" => Ok(CidToGidMap::Identity),
            p @ Primitive::Stream(_) | p @ Primitive::Reference(_) => {
                let stream: Stream<()> = Stream::from_primitive(p, resolve)?;
                let data = stream.data(resolve)?;
                Ok(CidToGidMap::Table(
                    data.chunks_exact(2)
                        .map(|c| (c[0] as u16) << 8 | c[1] as u16)
                        .collect(),
                ))
            }
            p => Err(PdfError::UnexpectedPrimitive {
                expected: "/Identity or Stream",
                found: p.get_debug_name(),
            }),
        }
    }
}
impl ObjectWrite for CidToGidMap {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            CidToGidMap::Identity => Ok(Name::from("Identity").into()),
            CidToGidMap::Table(ref table) => {
                let mut data = Vec::with_capacity(table.len() * 2);
                data.extend(
                    table
                        .iter()
                        .flat_map(|&v| <[u8; 2]>::into_iter(v.to_be_bytes())),
                );
                Stream::new((), data).to_primitive(update)
            }
        }
    }
}

impl Object for Font {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = p.resolve(resolve)?.into_dictionary()?;

        let subtype = t!(FontType::from_primitive(
            dict.require("Font", "Subtype")?,
            resolve
        ));

        // BaseFont is required for all FontTypes except Type3
        dict.expect("Font", "Type", "Font", true)?;
        let base_font_primitive = dict.get("BaseFont");
        let base_font = match (base_font_primitive, subtype) {
            (Some(name), _) => Some(t!(t!(name.clone().resolve(resolve)).into_name(), name)),
            (None, FontType::Type3) => None,
            (_, _) => {
                return Err(PdfError::MissingEntry {
                    typ: "Font",
                    field: "BaseFont".to_string(),
                })
            }
        };

        let encoding = dict
            .remove("Encoding")
            .map(|p| Object::from_primitive(p, resolve))
            .transpose()?;

        let to_unicode = match dict.remove("ToUnicode") {
            Some(p) => Some(Object::from_primitive(p, resolve)?),
            None => None,
        };
        let _other = dict.clone();
        let data = match subtype {
            FontType::Type0 => FontData::Type0(Type0Font::from_dict(dict, resolve)?),
            FontType::Type1 => FontData::Type1(TFont::from_dict(dict, resolve)?),
            FontType::TrueType => FontData::TrueType(TFont::from_dict(dict, resolve)?),
            FontType::CIDFontType0 => FontData::CIDFontType0(CIDFont::from_dict(dict, resolve)?),
            FontType::CIDFontType2 => FontData::CIDFontType2(CIDFont::from_dict(dict, resolve)?),
            _ => FontData::Other(dict),
        };

        Ok(Font {
            subtype,
            name: base_font,
            data,
            encoding,
            to_unicode,
            _other,
        })
    }
}
impl ObjectWrite for Font {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let mut dict = match self.data {
            FontData::CIDFontType0(ref d) | FontData::CIDFontType2(ref d) => d.to_dict(update)?,
            FontData::TrueType(ref d) | FontData::Type1(ref d) => d.to_dict(update)?,
            FontData::Type0(ref d) => d.to_dict(update)?,
            FontData::Other(ref dict) => dict.clone(),
        };

        if let Some(ref to_unicode) = self.to_unicode {
            dict.insert("ToUnicode", to_unicode.to_primitive(update)?);
        }
        if let Some(ref encoding) = self.encoding {
            dict.insert("Encoding", encoding.to_primitive(update)?);
        }
        if let Some(ref name) = self.name {
            dict.insert("BaseFont", name.to_primitive(update)?);
        }

        let subtype = match self.data {
            FontData::Type0(_) => FontType::Type0,
            FontData::Type1(_) => FontType::Type1,
            FontData::TrueType(_) => FontType::TrueType,
            FontData::CIDFontType0(_) => FontType::CIDFontType0,
            FontData::CIDFontType2(_) => FontType::CIDFontType2,
            FontData::Other(_) => bail!("unimplemented"),
        };
        dict.insert("Subtype", subtype.to_primitive(update)?);
        dict.insert("Type", Name::from("Font"));

        Ok(Primitive::Dictionary(dict))
    }
}

#[derive(Debug)]
pub struct Widths {
    values: Vec<f32>,
    default: f32,
    first_char: usize,
}
impl Widths {
    pub fn get(&self, cid: usize) -> f32 {
        if cid < self.first_char {
            self.default
        } else {
            self.values
                .get(cid - self.first_char)
                .cloned()
                .unwrap_or(self.default)
        }
    }
    fn new(default: f32) -> Widths {
        Widths {
            default,
            values: Vec::new(),
            first_char: 0,
        }
    }
    fn ensure_cid(&mut self, cid: usize) {
        if let Some(offset) = cid.checked_sub(self.first_char) {
            // cid may be < first_char
            // reserve difference of offset to capacity
            // if enough capacity to cover offset, saturates to zero, and reserve will do nothing
            self.values
                .reserve(offset.saturating_sub(self.values.capacity()));
        }
    }
    #[allow(clippy::float_cmp)] // TODO
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
            self.values
                .splice(0..0, repeat(self.default).take(self.first_char - cid));
            self.first_char = cid;
            self.values[0] = width;
            return;
        }

        if cid > self.values.len() + self.first_char {
            self.ensure_cid(cid);
            self.values
                .extend(repeat(self.default).take(cid - self.first_char - self.values.len()));
            self.values.push(width);
            return;
        }

        self.values[cid - self.first_char] = width;
    }
}
impl Font {
    pub fn embedded_data(&self, resolve: &impl Resolve) -> Option<Result<Arc<[u8]>>> {
        match self.data {
            FontData::Type0(ref t) => t
                .descendant_fonts
                .get(0)
                .and_then(|f| f.embedded_data(resolve)),
            FontData::CIDFontType0(ref c) | FontData::CIDFontType2(ref c) => {
                c.font_descriptor.data(resolve)
            }
            FontData::Type1(ref t) | FontData::TrueType(ref t) => {
                t.font_descriptor.as_ref().and_then(|d| d.data(resolve))
            }
            _ => None,
        }
    }
    pub fn is_cid(&self) -> bool {
        matches!(
            self.data,
            FontData::Type0(_) | FontData::CIDFontType0(_) | FontData::CIDFontType2(_)
        )
    }
    pub fn cid_to_gid_map(&self) -> Option<&CidToGidMap> {
        match self.data {
            FontData::Type0(ref inner) => inner
                .descendant_fonts
                .get(0)
                .and_then(|f| f.cid_to_gid_map()),
            FontData::CIDFontType0(ref f) | FontData::CIDFontType2(ref f) => {
                f.cid_to_gid_map.as_ref()
            }
            _ => None,
        }
    }
    pub fn encoding(&self) -> Option<&Encoding> {
        self.encoding.as_ref()
    }
    pub fn info(&self) -> Option<&TFont> {
        match self.data {
            FontData::Type1(ref info) => Some(info),
            FontData::TrueType(ref info) => Some(info),
            _ => None,
        }
    }
    pub fn widths(&self, resolve: &impl Resolve) -> Result<Option<Widths>> {
        match self.data {
            FontData::Type0(ref t0) => t0.descendant_fonts[0].widths(resolve),
            FontData::Type1(ref info) | FontData::TrueType(ref info) => match *info {
                TFont {
                    first_char: Some(first),
                    ref widths,
                    ..
                } => Ok(Some(Widths {
                    default: 0.0,
                    first_char: first as usize,
                    values: widths.as_ref().cloned().unwrap_or_default(),
                })),
                _ => Ok(None),
            },
            FontData::CIDFontType0(ref cid) | FontData::CIDFontType2(ref cid) => {
                let mut widths = Widths::new(cid.default_width);
                let mut iter = cid.widths.iter();
                while let Some(p) = iter.next() {
                    let c1 = p.as_usize()?;
                    match iter.next() {
                        Some(Primitive::Array(array)) => {
                            widths.ensure_cid(c1 + array.len() - 1);
                            for (i, w) in array.iter().enumerate() {
                                widths.set(c1 + i, w.as_number()?);
                            }
                        }
                        Some(&Primitive::Reference(r)) => match resolve.resolve(r)? {
                            Primitive::Array(array) => {
                                widths.ensure_cid(c1 + array.len() - 1);
                                for (i, w) in array.iter().enumerate() {
                                    widths.set(c1 + i, w.as_number()?);
                                }
                            }
                            p => {
                                return Err(PdfError::Other {
                                    msg: format!("unexpected primitive in W array: {:?}", p),
                                })
                            }
                        },
                        Some(&Primitive::Integer(c2)) => {
                            let w = try_opt!(iter.next()).as_number()?;
                            for c in c1..=(c2 as usize) {
                                widths.set(c, w);
                            }
                        }
                        p => {
                            return Err(PdfError::Other {
                                msg: format!("unexpected primitive in W array: {:?}", p),
                            })
                        }
                    }
                }
                Ok(Some(widths))
            }
            _ => Ok(None),
        }
    }
    pub fn to_unicode(&self, resolve: &impl Resolve) -> Option<Result<ToUnicodeMap>> {
        self.to_unicode
            .as_ref()
            .map(|s| (**s).data(resolve).and_then(|d| parse_cmap(&d)))
    }
}
#[derive(Object, ObjectWrite, Debug, DataSize, DeepClone)]
pub struct TFont {
    #[pdf(key = "BaseFont")]
    pub base_font: Option<Name>,

    /// per spec required, but some files lack it.
    #[pdf(key = "FirstChar")]
    pub first_char: Option<i32>,

    /// same
    #[pdf(key = "LastChar")]
    pub last_char: Option<i32>,

    #[pdf(key = "Widths")]
    pub widths: Option<Vec<f32>>,

    #[pdf(key = "FontDescriptor")]
    pub font_descriptor: Option<FontDescriptor>,
}

#[derive(Object, ObjectWrite, Debug, DataSize, DeepClone)]
pub struct Type0Font {
    #[pdf(key = "DescendantFonts")]
    pub descendant_fonts: Vec<MaybeRef<Font>>,

    #[pdf(key = "ToUnicode")]
    pub to_unicode: Option<RcRef<Stream<()>>>,
}

#[derive(Object, ObjectWrite, Debug, DataSize, DeepClone)]
pub struct CIDFont {
    #[pdf(key = "CIDSystemInfo")]
    pub system_info: Dictionary,

    #[pdf(key = "FontDescriptor")]
    pub font_descriptor: FontDescriptor,

    #[pdf(key = "DW", default = "1000.")]
    pub default_width: f32,

    #[pdf(key = "W")]
    pub widths: Vec<Primitive>,

    #[pdf(key = "CIDToGIDMap")]
    pub cid_to_gid_map: Option<CidToGidMap>,

    #[pdf(other)]
    pub _other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug, DataSize, DeepClone)]
pub struct FontDescriptor {
    #[pdf(key = "FontName")]
    pub font_name: Name,

    #[pdf(key = "FontFamily")]
    pub font_family: Option<PdfString>,

    #[pdf(key = "FontStretch")]
    pub font_stretch: Option<FontStretch>,

    #[pdf(key = "FontWeight")]
    pub font_weight: Option<f32>,

    #[pdf(key = "Flags")]
    pub flags: u32,

    #[pdf(key = "FontBBox")]
    pub font_bbox: Rectangle,

    #[pdf(key = "ItalicAngle")]
    pub italic_angle: f32,

    // required as per spec, but still missing in some cases
    #[pdf(key = "Ascent")]
    pub ascent: Option<f32>,

    #[pdf(key = "Descent")]
    pub descent: Option<f32>,

    #[pdf(key = "Leading", default = "0.")]
    pub leading: f32,

    #[pdf(key = "CapHeight")]
    pub cap_height: Option<f32>,

    #[pdf(key = "XHeight", default = "0.")]
    pub xheight: f32,

    #[pdf(key = "StemV", default = "0.")]
    pub stem_v: f32,

    #[pdf(key = "StemH", default = "0.")]
    pub stem_h: f32,

    #[pdf(key = "AvgWidth", default = "0.")]
    pub avg_width: f32,

    #[pdf(key = "MaxWidth", default = "0.")]
    pub max_width: f32,

    #[pdf(key = "MissingWidth", default = "0.")]
    pub missing_width: f32,

    #[pdf(key = "FontFile")]
    pub font_file: Option<RcRef<Stream<()>>>,

    #[pdf(key = "FontFile2")]
    pub font_file2: Option<RcRef<Stream<()>>>,

    #[pdf(key = "FontFile3")]
    pub font_file3: Option<RcRef<Stream<FontStream3>>>,

    #[pdf(key = "CharSet")]
    pub char_set: Option<PdfString>,
}
impl FontDescriptor {
    pub fn data(&self, resolve: &impl Resolve) -> Option<Result<Arc<[u8]>>> {
        if let Some(ref s) = self.font_file {
            Some((**s).data(resolve))
        } else if let Some(ref s) = self.font_file2 {
            Some((**s).data(resolve))
        } else if let Some(ref s) = self.font_file3 {
            Some((**s).data(resolve))
        } else {
            None
        }
    }
}

#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
#[pdf(key = "Subtype")]
pub enum FontTypeExt {
    Type1C,
    CIDFontType0C,
    OpenType,
}
#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
pub struct FontStream3 {
    #[pdf(key = "Subtype")]
    pub subtype: FontTypeExt,
}

#[derive(
    Object, ObjectWrite, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, DataSize, DeepClone,
)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

#[derive(Clone, Debug, Default)]
pub struct ToUnicodeMap {
    // todo: reduce allocations
    inner: HashMap<u16, SmallString>,
}
impl ToUnicodeMap {
    pub fn new() -> Self {
        Self::default()
    }
    /// Create a new ToUnicodeMap from key/value pairs.
    ///
    /// subject to change
    pub fn create(iter: impl Iterator<Item = (u16, SmallString)>) -> Self {
        ToUnicodeMap {
            inner: iter.collect(),
        }
    }
    pub fn get(&self, gid: u16) -> Option<&str> {
        self.inner.get(&gid).map(|s| s.as_str())
    }
    pub fn insert(&mut self, gid: u16, unicode: SmallString) {
        self.inner.insert(gid, unicode);
    }
    pub fn iter(&self) -> impl Iterator<Item = (u16, &str)> {
        self.inner
            .iter()
            .map(|(&gid, unicode)| (gid, unicode.as_str()))
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// helper function to decode UTF-16-BE data
/// takes a slice of u8 and returns an iterator for char or an decoding error
pub fn utf16be_to_char(
    data: &[u8],
) -> impl Iterator<Item = std::result::Result<char, std::char::DecodeUtf16Error>> + '_ {
    char::decode_utf16(
        data.chunks_exact(2)
            .map(|w| u16::from_be_bytes([w[0], w[1]])),
    )
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
                if a.is_err() {
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
                if a.is_err() {
                    break;
                }
                let b = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::STRING);
                let c = parse_with_lexer(
                    &mut lexer,
                    &NoResolve,
                    ParseFlags::STRING | ParseFlags::ARRAY,
                );
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

fn write_cid(w: &mut String, cid: u16) {
    write!(w, "<{:04X}>", cid).unwrap();
}
fn write_unicode(out: &mut String, unicode: &str) {
    let mut buf = [0; 2];
    write!(out, "<").unwrap();
    for c in unicode.chars() {
        let slice = c.encode_utf16(&mut buf);
        for &word in slice.iter() {
            write!(out, "{:04X}", word).unwrap();
        }
    }
    write!(out, ">").unwrap();
}
pub fn write_cmap(map: &ToUnicodeMap) -> String {
    let mut buf = String::new();
    let mut list: Vec<(u16, &str)> = map
        .inner
        .iter()
        .map(|(&cid, s)| (cid, s.as_str()))
        .collect();
    list.sort();

    let mut remaining = &list[..];
    let blocks = std::iter::from_fn(move || {
        if remaining.len() == 0 {
            return None;
        }
        let first_cid = remaining[0].0;
        let seq_len = remaining
            .iter()
            .enumerate()
            .take_while(|&(i, &(cid, _))| cid == first_cid + i as u16)
            .count();

        let (block, tail) = remaining.split_at(seq_len);
        remaining = tail;
        Some(block)
    });

    for (single, group) in &blocks.chunk_by(|b| b.len() == 1) {
        if single {
            writeln!(buf, "beginbfchar").unwrap();
            for block in group {
                for &(cid, uni) in block {
                    write_cid(&mut buf, cid);
                    write!(buf, " ").unwrap();
                    write_unicode(&mut buf, uni);
                    writeln!(buf).unwrap();
                }
            }
            writeln!(buf, "endbfchar").unwrap();
        } else {
            writeln!(buf, "beginbfrange").unwrap();
            for block in group {
                write_cid(&mut buf, block[0].0);
                write!(buf, " ").unwrap();
                write_cid(&mut buf, block.last().unwrap().0);
                write!(buf, " [").unwrap();
                for (i, &(_cid, u)) in block.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ").unwrap();
                    }
                    write_unicode(&mut buf, u);
                }
                writeln!(buf, "]").unwrap();
            }
            writeln!(buf, "endbfrange").unwrap();
        }
    }

    buf
}

#[cfg(test)]
mod tests {

    use crate::font::{utf16be_to_char, utf16be_to_string, utf16be_to_string_lossy};
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
            0xD8, 0x34, 0xDD, 0x1E, 0x00, 0x6d, 0x00, 0x75, 0x00, 0x73, 0xDD, 0x1E, 0x00, 0x69,
            0x00, 0x63, 0xD8, 0x34,
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
