use crate::object::*;
use crate::primitive::*;
use crate::error::*;
use crate::encoding::Encoding;
use std::io;
use std::rc::Rc;

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
    pub name: String,
    pub data: FontData,
    
    encoding: Encoding,
    
    to_unicode: Option<Stream>,
    
    _other: Dictionary
}

#[derive(Debug)]
pub enum FontData {
    Type1(TFont),
    Type0(Type0Font),
    TrueType(TFont),
    CIDFontType0(CIDFont),
    CIDFontType2(CIDFont),
    Other(Dictionary),
    Standard(&'static str)
}

pub static STANDARD_FONTS: &[(&'static str, &'static str)] = &[
    ("Courier", "CourierStd.otf"),
    ("Courier-Bold", "CourierStd-Bold.otf"),
    ("Courier-Oblique", "CourierStd-Oblique.otf"),
    ("Courier-BoldOblique", "CourierStd-BoldOblique.otf"),
    
    ("Times-Roman", "MinionPro-Regular.otf"),
    ("Times-Bold", "MinionPro-Bold.otf"),
    ("Times-Italic", "MinionPro-It.otf"),
    ("Times-BoldItalic", "MinionPro-BoldIt.otf"),
    
    ("Helvetica", "MyriadPro-Regular.otf"),
    ("Helvetica-Bold", "MyriadPro-Bold.otf"),
    ("Helvetica-Oblique", "MyriadPro-It.otf"),
    ("Helvetica-BoldOblique", "MyriadPro-BoldIt.otf"),
    
    ("Symbol", "SY______.PFB"),
    ("ZapfDingbats", "AdobePiStd.otf"),
    
    ("Arial-BoldMT", "Arial-BoldMT.otf"),
    ("ArialMT", "ArialMT.ttf"),
    ("Arial-ItalicMT", "Arial-ItalicMT.otf"),
];
impl Object for Font {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {unimplemented!()}
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = p.to_dictionary(resolve)?;
        dict.expect("Font", "Type", "Font", true)?;
        let base_font = dict.require("Font", "BaseFont")?.to_name()?;
        let subtype = FontType::from_primitive(dict.require("Font", "Subtype")?, resolve)?;
        
        let encoding = match dict.remove("Encoding") {
            Some(p) => Object::from_primitive(p, resolve)?,
            None => Encoding::standard()
        };
        let to_unicode = match dict.remove("ToUnicode") {
            Some(p) => Some(Stream::from_primitive(p, resolve)?),
            None => None
        };
        let _other = dict.clone();
        let data = match STANDARD_FONTS.iter().filter(|&(name, _)| *name == base_font).next() {
            Some((_, filename)) => {
                FontData::Standard(filename)
            }
            None => match subtype {
                FontType::Type0 => FontData::Type0(Type0Font::from_dict(dict, resolve)?),
                FontType::Type1 => FontData::Type1(TFont::from_dict(dict, resolve)?),
                FontType::TrueType => FontData::TrueType(TFont::from_dict(dict, resolve)?),
                FontType::CIDFontType0 => FontData::CIDFontType0(CIDFont::from_dict(dict, resolve)?),
                FontType::CIDFontType2 => FontData::CIDFontType2(CIDFont::from_dict(dict, resolve)?),
                _ => FontData::Other(dict)
            }
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

impl Font {
    pub fn standard_font(&self) -> Option<&str> {
        match self.data {
            FontData::Standard(name) => Some(name),
            _ => None
        }
    }
    pub fn embedded_data(&self) -> Option<Result<&[u8]>> {
        match self.data {
            FontData::Type0(ref t) => t.descendant_fonts.get(0).and_then(|f| f.embedded_data()),
            FontData::CIDFontType0(ref c) | FontData::CIDFontType2(ref c) => c.font_descriptor.data(),
            FontData::Type1(ref t) | FontData::TrueType(ref t) => t.font_descriptor.data(),
            _ => None
        }
    }
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }
    pub fn info(&self) -> Option<&TFont> {
        match self.data {
            FontData::Type1(ref info) => Some(info),
            FontData::TrueType(ref info) => Some(info),
            _ => None
        }
    }
    pub fn widths(&self) -> Result<Option<[f32; 256]>> {
        match self.data {
            FontData::Type0(ref t0) => t0.descendant_fonts[0].widths(),
            FontData::Type1(ref info) | FontData::TrueType(ref info) => {
                let mut widths = [0.0; 256];
                widths[info.first_char as usize .. info.first_char as usize + info.widths.len()]
                    .copy_from_slice(&info.widths);
                Ok(Some(widths))
            },
            FontData::CIDFontType0(ref cid) | FontData::CIDFontType2(ref cid) => {
                let mut widths = [cid.default_width; 256];
                let mut iter = cid.widths.iter();
                while let Some(ref p) = iter.next() {
                    let c1 = p.as_integer()? as usize;
                    match iter.next() {
                        Some(&Primitive::Array(ref array)) => {
                            for (i, w) in array.iter().enumerate() {
                                widths[c1 + i] = w.as_number()?;
                            }
                        },
                        Some(&Primitive::Integer(c2)) => {
                            let w = iter.next()?.as_number()?;
                            for c in (c1 as usize) ..= (c2 as usize) {
                                widths[c] = w;
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
}
#[derive(Object, Debug)]
pub struct TFont {
    #[pdf(key="Name")]
    pub name: Option<String>,
    
    #[pdf(key="FirstChar")]
    pub first_char: i32,
    
    #[pdf(key="LastChar")]
    pub last_char: i32,
    
    #[pdf(key="Widths")]
    pub widths: Vec<f32>,
    
    #[pdf(key="FontDescriptor")]
    font_descriptor: FontDescriptor
}

#[derive(Object, Debug)]
pub struct Type0Font {
    #[pdf(key="DescendantFonts")]
    descendant_fonts: Vec<Rc<Font>>,
    
    #[pdf(key="ToUnicode")]
    to_unicode: Option<Stream>,
}

#[derive(Object, Debug)]
pub struct CIDFont {
    #[pdf(key="CIDSystemInfo")]
    system_info: Dictionary,
    
    #[pdf(key="FontDescriptor")]
    font_descriptor: FontDescriptor,
    
    #[pdf(key="DW")]
    default_width: f32,
    
    #[pdf(key="W")]
    pub widths: Vec<Primitive>,

    #[pdf(key="CIDToGIDMap")]
    map: Primitive,
    
    #[pdf(other)]
    _other: Dictionary
}


#[derive(Object, Debug)]
pub struct FontDescriptor {
    #[pdf(key="FontName")]
    font_name: String,
    
    #[pdf(key="FontFamily")]
    font_family: Option<PdfString>,
    
    #[pdf(key="FontStretch")]
    font_stretch: Option<FontStretch>,

    #[pdf(key="FontWeight")]
    font_weight: Option<f32>,
    
    #[pdf(key="Flags")]
    flags: u32,
    
    #[pdf(key="FontBBox")]
    font_bbox: Rect,
    
    #[pdf(key="ItalicAngle")]
    italic_angle: f32,
    
    #[pdf(key="Ascent")]
    ascent: f32,
    
    #[pdf(key="Descent")]
    descent: f32,
    
    #[pdf(key="Leading", default="0.")]
    leading: f32,
    
    #[pdf(key="CapHeight")]
    cap_height: f32,
    
    #[pdf(key="XHeight", default="0.")]
    xheight: f32,
    
    #[pdf(key="StemV", default="0.")]
    stem_v: f32,
    
    #[pdf(key="StemH", default="0.")]
    stem_h: f32,
    
    #[pdf(key="AvgWidth", default="0.")]
    avg_width: f32,
    
    #[pdf(key="MaxWidth", default="0.")]
    max_width: f32,
    
    #[pdf(key="MissingWidth", default="0.")]
    missing_width: f32,
    
    #[pdf(key="FontFile")]
    font_file: Option<Stream>,
    
    #[pdf(key="FontFile2")]
    font_file2: Option<Stream>,
    
    #[pdf(key="FontFile3")]
    font_file3: Option<Stream<FontStream3>>,
    
    #[pdf(key="CharSet")]
    char_set: Option<PdfString>
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
enum FontTypeExt {
    Type1C,
    CIDFontType0C,
    OpenType
}
#[derive(Object, Debug, Clone)]
struct FontStream3 {
    #[pdf(key="Subtype")]
    subtype: FontTypeExt
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
