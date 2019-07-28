use std::error::Error;
use std::collections::HashMap;
use pathfinder_geometry::transform2d::Transform2F;
use crate::{Font, BorrowedFont, Glyph, Glyphs, Value, Context, State, type1, type2, IResultExt, R};
use nom::{
    number::complete::{be_u8, be_i8, be_u16, be_i16, be_u24, be_u32, be_i32},
    bytes::complete::{take},
    multi::{count, many0},
    combinator::map,
    sequence::tuple,
    error::{make_error, ErrorKind},
    Err::*,
};
use encoding::{Encoding};

pub struct CffFont<'a> {
    private_dict: HashMap<Operator, Vec<Value>>,
    char_strings: Index<'a>,
    char_string_type: CharstringType,
    context: Context<'a>,
    font_matrix: Transform2F,
    codepoint_map: [u16; 256],  // codepoint -> glyph index
    name_map: HashMap<&'a str, u16>,
    encoding: Option<Encoding>
}

impl<'a> CffFont<'a> {
    pub fn parse(data: &'a [u8], idx: u32) -> Self {
        match read_cff(data) {
            Ok((_, cff)) => {
                cff.parse_font(idx)
            },
            Err(Incomplete(_)) => panic!("need more data"),
            Err(Error(v)) | Err(Failure(v)) => {
                for (i, e) in v.errors {
                    println!("{:?} {:?}", &i[.. i.len().min(20)], e);
                }
                panic!()
            }
        }
    }
}
impl<'a> Font for CffFont<'a> {
    fn num_glyphs(&self) -> u32 {
        self.char_strings.len()
    }
    fn font_matrix(&self) -> Transform2F {
        self.font_matrix
    }
    fn glyph(&self, id: u32) -> Result<Glyph, Box<dyn Error>> {
        let mut state = State::new();
        debug!("charstring for glyph {}", id);
        let data = self.char_strings.get(id).expect("no charstring for glyph");
        match self.char_string_type {
            CharstringType::Type1 => {
                type1::charstring(data, &self.context, &mut state).expect("faild to parse charstring");
            },
            CharstringType::Type2 => {
                type2::charstring(data, &self.context, &mut state).expect("faild to parse charstring");
            }
        }
        debug!("glyph {} {:?} {:?}", id, state.char_width, state.delta_width);
        let width = match (state.char_width, state.delta_width) {
            (Some(w), None) => w,
            (None, None) =>
                self.private_dict.get(&Operator::DefaultWidthX).map(|a| a[0].to_float()).unwrap_or(0.),
            (None, Some(delta)) =>
                delta + self.private_dict.get(&Operator::NominalWidthX).map(|a| a[0].to_float()).unwrap_or(0.),
            (Some(_), Some(_)) => panic!("BUG: both char_width and delta_width set")
        };
        Ok(Glyph {
            width,
            path: state.into_path()
        })
    }
    fn gid_for_codepoint(&self, codepoint: u32) -> Option<u32> {
        match self.codepoint_map.get(codepoint as usize) {
            None | Some(&0) => None,
            Some(&n) => Some(n as u32 + 1)
        }
    }
    fn gid_for_name(&self, name: &str) -> Option<u32> {
        match self.name_map.get(name) {
            None => None,
            Some(&gid) => Some(gid as u32 + 1)
        }
    }
    fn encoding(&self) -> Option<Encoding> {
        self.encoding
    }
}
impl<'a> BorrowedFont<'a> for CffFont<'a> {}

pub fn read_cff(data: &[u8]) -> R<Cff> {
    let i = data;
    let (i, major) = be_u8(i)?;
    assert_eq!(major, 1);
    let (i, _minor) = be_u8(i)?;
    
    let (i, hdrSize) = be_u8(i)?;
    let (i, _offSize) = be_u8(i)?;
    let (i, _) = take(hdrSize - 4)(i)?;
    
    println!("name_index");
    let (i, name_index) = index(i)?;
    
    println!("dict_index");
    let (i, dict_index) = index(i)?;
    
    println!("string_index");
    let (i, string_index) = index(i)?;
    
    println!("subroutines");
    let (i, subroutines) = index(i)?;
    
    Ok((i, Cff {
        data,
        name_index,
        dict_index,
        string_index,
        subroutines
    }))
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
pub struct Cff<'a> {
    data: &'a [u8],
    name_index: Index<'a>,
    dict_index: Index<'a>,
    string_index: Index<'a>,
    subroutines: Index<'a>,
}

impl<'a> Cff<'a> {
    fn parse_font(&self, idx: u32) -> CffFont<'a> {
        let data = self.dict_index.get(idx).expect("font not found");
        let top_dict = dict(data).unwrap().1;
        info!("top dict: {:?}", top_dict);
        
        let font_matrix = top_dict.get(&Operator::FontMatrix)
            .map(|arr| Transform2F::row_major(
                arr[0].into(), arr[1].into(), arr[2].into(),
                arr[3].into(), arr[4].into(), arr[5].into()))
            .unwrap_or(Transform2F::row_major(0.001, 0., 0., 0.001, 0., 0.));
        
        let offset = top_dict[&Operator::CharStrings][0].to_int() as usize;
        let char_strings = index(self.data.get(offset ..).unwrap()).get();
        let num_glyphs = char_strings.len() as usize;
        
        let n = top_dict.get(&Operator::CharstringType).map(|v| v[0].to_int()).unwrap_or(2);
        let char_string_type = match n {
            1 => CharstringType::Type1,
            2 => CharstringType::Type2,
            _ => panic!("invalid charstring type")
        };
        
        let charset_offset = top_dict[&Operator::Charset][0].to_int() as usize;
        let charset = charset(self.data.get(charset_offset ..).unwrap(), num_glyphs).get();
        
        let glyph_name = |sid: SID|
            STANDARD_STRINGS.get(sid as usize).cloned().unwrap_or_else(||
                ::std::str::from_utf8(self.string_index.get(sid as u32 - STANDARD_STRINGS.len() as u32).expect("no such string")).expect("Invalid glyph name")
            );
        
        // gid(index) -> sid
        let sids: Vec<SID> = match charset {
            Charset::Continous(sids) => sids,
            Charset::Ranges(ranges) => ranges.into_iter()
                .flat_map(|(sid, num)| (sid .. sid + num + 1))
                .collect(),
        };
        let sid_map: HashMap<SID, u16> = sids.iter().enumerate()
            .map(|(gid, &sid)| (sid as u16, gid as u16))
            .collect();
        
        let name_map: HashMap<_, _> = sids.iter().enumerate()
            .map(|(gid, &sid)| (glyph_name(sid), gid as u16))
            .collect();
        
        let build_default = |sids: &[SID; 256]| -> [u16; 256] {
            let mut cmap = [0u16; 256];
            for (codepoint, sid) in sids.iter().enumerate() {
                if let Some(&gid) = sid_map.get(sid) {
                    cmap[codepoint as usize] = gid;
                }
            }
            cmap
        };
        
        let (cmap, encoding) = match top_dict.get(&Operator::Encoding).map(|a| a[0].to_int()) {
            None | Some(0)
                => (build_default(&STANDARD_ENCODING), Some(Encoding::AdobeStandard)),
            Some(1)
                => (build_default(&EXPERT_ENCODING), Some(Encoding::AdobeExpert)),
            Some(offset) => {
                let mut cmap = [0u16; 256];
                let (codepoints, supplement) = encoding(self.data.get(offset as _ ..).unwrap()).get();
                match codepoints {
                    GlyphEncoding::Continous(codepoints) => codepoints.iter()
                        .enumerate().for_each(|(gid, &codepoint)| cmap[codepoint as usize] = gid as _),
                    GlyphEncoding::Ranges(ranges) => ranges.iter()
                        .flat_map(|&(first, left)| (first ..).take(left as usize + 1))
                        .enumerate().for_each(|(gid, codepoint)| cmap[codepoint as usize] = gid as _),
                }
                supplement.iter()
                    .for_each(|&(codepoint, sid)| cmap[codepoint as usize] = sid_map[&sid]);
                    
                (cmap, None)
            }
        };
        debug!("cmap:");
        for (i, &gid) in cmap.iter().enumerate() {
            if gid != 0 {
                let sid = sids[gid as usize];
                debug!("{} -> gid={}, sid={}, name={}", i, gid, sid, glyph_name(sid));
            }
        }
        /*
        let transform = reverse_map(
        */
        let private_dict_entry = top_dict.get(&Operator::Private)
            .expect("no private dict entry");
        
        let private_dict_size = private_dict_entry[0].to_int() as usize;
        let private_dict_offset = private_dict_entry[1].to_int() as usize;
        let private_dict_data = &self.data[private_dict_offset .. private_dict_offset + private_dict_size];
        let private_dict = dict(private_dict_data).get();
        info!("private dict: {:?}", private_dict);
        
        let subrs = private_dict.get(&Operator::Subrs).map(|arr| {
            let private_subroutines_offset = arr[0].to_int() as usize;
            index(&self.data[(private_dict_offset + private_subroutines_offset) as usize ..])
                .get().items
                .into_iter().map(|i| i.into())
                .collect()
        }).unwrap_or(vec![]);
        
        let subr_bias = match char_string_type {
            CharstringType::Type2 => bias(subrs.len()),
            CharstringType::Type1 => 0
        };
        let global_subrs = self.subroutines.iter()
            .map(|data| data.into())
            .collect();
        
        let global_subr_bias = match char_string_type {
            CharstringType::Type2 => bias(self.subroutines.len() as usize),
            CharstringType::Type1 => 0
        };

        let context = Context {
            subr_bias,
            subrs,
            global_subrs,
            global_subr_bias
        };
        
        CffFont {
            private_dict,
            char_strings,
            char_string_type,
            context,
            font_matrix,
            codepoint_map: cmap,
            name_map,
            encoding
        }
    }
}

fn dict(mut input: &[u8]) -> R<HashMap<Operator, Vec<Value>>> {
    let mut map = HashMap::new();
    while input.len() > 0 {
        let (i, args) = many0(value)(input)?;
        let (i, key) = operator(i)?;
        map.insert(key, args);
        
        input = i;
    }

    Ok((input, map))
}

enum CharstringType {
    Type1,
    Type2
}

pub struct Index<'a> {
    items: Vec<&'a [u8]>
}
impl<'a> Index<'a> {
    pub fn get(&self, idx: u32) -> Option<&'a [u8]> {
        self.items.get(idx as usize).cloned()
    }
    pub fn iter<'b>(&'b self) -> impl Iterator<Item=&'a [u8]> + 'b {
        self.items.iter().cloned()
    }
    pub fn len(&self) -> u32 {
        self.items.len() as u32
    }
}
    
fn index(i: &[u8]) -> R<Index> {
    let (i, n) = map(be_u16, |n| n as usize)(i)?;
    debug!("n={}", n);
    if n != 0 {
        let (i, offSize) = be_u8(i)?;
        let (i, offsets) = count(map(offset(offSize), |o| o - 1), n+1)(i)?;
        let (i, data) = take(offsets[n])(i)?;
        
        let items = offsets.windows(2).map(|w| data.get(w[0] as usize .. w[1] as usize).unwrap()).collect();
        Ok((i, Index {
            items
        }))
    } else {
        Ok((i, Index { items: vec![] }))
    }
}

fn offset(size: u8) -> impl Fn(&[u8]) -> R<u32> {
    move |i| match size {
        1 => map(be_u8, |n| n as u32)(i),
        2 => map(be_u16, |n| n as u32)(i),
        3 => be_u24(i),
        4 => be_u32(i),
        _ => Err(Failure(make_error(i, ErrorKind::TooLarge)))
    }
}

fn float(data: &[u8]) -> R<f32> {
    let mut pos = 0;
    let mut next_nibble = || -> u8 {
        let nibble = (data[pos/2] >> (4 * (pos & 1) as u8)) & 0xf;
        pos += 1;
        nibble
    };
    
    let mut is_negaive = false;
    let mut num_digits = 0;
    let mut n: i32 = 0;
    let mut p: i32 = 0;
    let mut power_negative = false;
    let mut decimal_point = None;
    loop {
        match next_nibble() {
            d @ 0 ..= 9 => {
                n = 10 * n + d as i32;
                num_digits += 1;
            }
            0xa => decimal_point = Some(num_digits),
            b @ 0xb | b @ 0xc  => { // positive 10^x
                power_negative = b == 0xc;
                loop {
                    match next_nibble() {
                        d @ 0 ..= 9 => p = 10 * p + d as i32,
                        0xf => break,
                        _ => panic!("invalid float")
                    }
                }
            },
            0xd => panic!("reserved"),
            0xe => is_negaive = true,
            0xf => break,
            _ => unreachable!()
        }
    }
    
    if is_negaive {
        n *= -1;
    }
    let mut value = n as f32;
    let mut power = 0;
    if let Some(dp) = decimal_point {
        power += dp - num_digits;
    }
    if p != 0 {
        if power_negative {
            p *= -1;
        }
        power += p;
    }
    if power != 0 {
        value *= 10.0f32.powi(power);
    }
    Ok((&data[(pos+1)/2 ..], value))
}


fn value(input: &[u8]) -> R<Value> {
    let (i, b0) = be_u8(input)?;
    
    match b0 {
        22 ..= 27 => panic!("reserved"),
        28 => map(be_i16, |n| n.into())(i),
        29 => map(be_i32, |n| n.into())(i),
        30 => map(float, |f| f.into())(i),
        31 => panic!("reserved"),
        b0 @ 32 ..= 246 => Ok((i, (b0 as i32 - 139).into())),
        b0 @ 247 ..= 250 => map(be_u8, |b1| ((b0 as i32 - 247) * 256 + b1 as i32 + 108).into())(i),
        b0 @ 251 ..= 254 => map(be_u8, |b1| (-(b0 as i32 - 251) * 256 - b1 as i32 - 108).into())(i),
        255 => panic!("reserved"),
        _ => Err(Error(make_error(input, ErrorKind::TooLarge))) 
    }
}

#[allow(dead_code)] 
#[derive(Debug, PartialEq, Eq, Hash)]
enum Operator {
    Version,
    Notice,
    Copyleft,
    FullName,
    FamilyName,
    Weight,
    IsFixedPitch,
    ItalicAngle,
    UnderlinePosition,
    UnderlineThickness,
    PaintType,
    CharstringType,
    FontMatrix,
    UniqueID,
    FontBBox,
    StrokeWidth,
    XUID,
    Charset,
    Encoding,
    CharStrings,
    Private,
    SyntheticBase,
    PostScript,
    BaseFontName,
    BaseFontBlend,
    ROS,
    CIDFontVersion,
    CIDFontRevision,
    CIDFontType,
    CIDCount,
    UIDBase,
    FDArray,
    
    BlueValues,
    OtherBlues,
    FamilyBlues,
    FamilyOtherBlues,
    BlueScale,
    BlueShift,
    BlueFuzz,
    StdHW,
    StdVW,
    StemSnapH,
    StemSnapV,
    ForceBold,
    LanguageGroup,
    ExpansionFactor,
    InitialRandomSeed,
    Subrs,
    DefaultWidthX,
    NominalWidthX
}

fn operator(input: &[u8]) -> R<Operator> {
    use Operator::*;
    
    let (i, b) = be_u8(input)?;
    let (i, v) = match b {
        0 => (i, Version),
        1 => (i, Notice),
        2 => (i, FullName),
        3 => (i, FamilyName),
        4 => (i, Weight),
        5 => (i, FontBBox),
        6 => (i, BlueValues),
        7 => (i, OtherBlues),
        8 => (i, FamilyBlues),
        9 => (i, FamilyOtherBlues),
        10 => (i, StdHW),
        11 => (i, StdVW),
        12 => {
            let (i, b) = be_u8(i)?;
            match b {
                0 => (i, Copyleft),
                1 => (i, IsFixedPitch),
                2 => (i, ItalicAngle),
                3 => (i, UnderlinePosition),
                4 => (i, UnderlineThickness),
                5 => (i, PaintType),
                6 => (i, CharstringType),
                7 => (i, FontMatrix),
                8 => (i, StrokeWidth),
                9 => (i, BlueScale),
                10 => (i, BlueShift),
                11 => (i, BlueFuzz),
                12 => (i, StemSnapH),
                13 => (i, StemSnapV),
                14 => (i, ForceBold),
                17 => (i, LanguageGroup),
                18 => (i, ExpansionFactor),
                19 => (i, InitialRandomSeed),
                20 => (i, SyntheticBase),
                21 => (i, PostScript),
                22 => (i, BaseFontName),
                23 => (i, BaseFontBlend),
                30 => (i, ROS),
                31 => (i, CIDFontVersion),
                32 => (i, CIDFontRevision),
                33 => (i, CIDFontType),
                34 => (i, CIDCount),
                35 => (i, UIDBase),
                36 => (i, FDArray),
                _ => return Err(nom::Err::Failure(make_error(input, ErrorKind::TooLarge)))
            }
        }
        13 => (i, UniqueID),
        14 => (i, XUID),
        15 => (i, Charset),
        16 => (i, Encoding),
        17 => (i, CharStrings),
        18 => (i, Private),
        19 => (i, Subrs),
        20 => (i, DefaultWidthX),
        21 => (i, NominalWidthX),
        _ => return Err(nom::Err::Failure(make_error(input, ErrorKind::TooLarge)))
    };
    Ok((i, v))
}

type Card8 = u8;
type Card16 = u16;
type OffSize = u8;
type SID = u16;

#[derive(Debug)]
enum Charset {
    Continous(Vec<SID>),
    Ranges(Vec<(SID, u16)>), // start, num-1
}

fn ranges<'a, F>(count_parser: F, num_glyphs: usize) -> impl Fn(&'a [u8]) -> R<'a, Vec<(SID, u16)>> where
    F: Fn(&'a [u8])-> R<'a, u16>
{
    move |mut input: &[u8]| {
        let mut total = 0;
        let mut vec = Vec::new();
        loop {
            let (i, sid) = be_u16(input)?;
            let (i, n) = count_parser(i)?;
            vec.push((sid, n));
            
            total += n as usize + 1;
            input = i;
            
            if total >= num_glyphs - 1 {
                break;
            }
        }
        Ok((input, vec))
    }
}
fn charset(i: &[u8], num_glyphs: usize) -> R<Charset> {
    let (i, format) = be_u8(i)?;
    
    match format {
        0 => {
            map(count(be_u16, num_glyphs as usize - 1), |a| Charset::Continous(a))(i)
        },
        1 => {
            map(ranges(map(be_u8, |n| n as u16), num_glyphs), |r| Charset::Ranges(r))(i)
        }
        2 => {
            map(ranges(be_u16, num_glyphs), |r| Charset::Ranges(r))(i)
        },
        _ => panic!("invalid charset format")
    }
}

#[derive(Debug)]
enum GlyphEncoding {
    Continous(Vec<u8>),
    Ranges(Vec<(u8, u8)>), // start, num-1
}

fn encoding(i: &[u8]) -> R<(GlyphEncoding, Vec<(u8, SID)>)> {
    let (i, format) = be_u8(i)?;
    
    let (i, encoding) = match format & 0x7F {
        0 => {
            let (i, num) = be_u8(i)?;
            map(count(be_u8, num as usize), |a| GlyphEncoding::Continous(a))(i)?
        },
        1 => {
            let (i, num) = be_u8(i)?;
            map(count(tuple((be_u8, be_u8)), num as usize), |a| GlyphEncoding::Ranges(a))(i)?
        }
        _ => panic!("invalid charset format")
    };
    if format & 0x80 != 0 {
        let (i, n) = be_u8(i)?;
        let (i, supplement) = count(tuple((be_u8, be_u16)), n as usize)(i)?;
        Ok((i, (encoding, supplement)))
    } else {
        Ok((i, (encoding, vec![])))
    }
}

static STANDARD_STRINGS: [&'static str; 391] = [
/*   0 */ ".notdef",
/*   1 */ "space",
/*   2 */ "exclam",
/*   3 */ "quotedbl",
/*   4 */ "numbersign",
/*   5 */ "dollar",
/*   6 */ "percent",
/*   7 */ "ampersand",
/*   8 */ "quoteright",
/*   9 */ "parenleft",
/*  10 */ "parenright",
/*  11 */ "asterisk",
/*  12 */ "plus",
/*  13 */ "comma",
/*  14 */ "hyphen",
/*  15 */ "period",
/*  16 */ "slash",
/*  17 */ "zero",
/*  18 */ "one",
/*  19 */ "two",
/*  20 */ "three",
/*  21 */ "four",
/*  22 */ "five",
/*  23 */ "six",
/*  24 */ "seven",
/*  25 */ "eight",
/*  26 */ "nine",
/*  27 */ "colon",
/*  28 */ "semicolon",
/*  29 */ "less",
/*  30 */ "equal",
/*  31 */ "greater",
/*  32 */ "question",
/*  33 */ "at",
/*  34 */ "A",
/*  35 */ "B",
/*  36 */ "C",
/*  37 */ "D",
/*  38 */ "E",
/*  39 */ "F",
/*  40 */ "G",
/*  41 */ "H",
/*  42 */ "I",
/*  43 */ "J",
/*  44 */ "K",
/*  45 */ "L",
/*  46 */ "M",
/*  47 */ "N",
/*  48 */ "O",
/*  49 */ "P",
/*  50 */ "Q",
/*  51 */ "R",
/*  52 */ "S",
/*  53 */ "T",
/*  54 */ "U",
/*  55 */ "V",
/*  56 */ "W",
/*  57 */ "X",
/*  58 */ "Y",
/*  59 */ "Z",
/*  60 */ "bracketleft",
/*  61 */ "backslash",
/*  62 */ "bracketright",
/*  63 */ "asciicircum",
/*  64 */ "underscore",
/*  65 */ "quoteleft",
/*  66 */ "a",
/*  67 */ "b",
/*  68 */ "c",
/*  69 */ "d",
/*  70 */ "e",
/*  71 */ "f",
/*  72 */ "g",
/*  73 */ "h",
/*  74 */ "i",
/*  75 */ "j",
/*  76 */ "k",
/*  77 */ "l",
/*  78 */ "m",
/*  79 */ "n",
/*  80 */ "o",
/*  81 */ "p",
/*  82 */ "q",
/*  83 */ "r",
/*  84 */ "s",
/*  85 */ "t",
/*  86 */ "u",
/*  87 */ "v",
/*  88 */ "w",
/*  89 */ "x",
/*  90 */ "y",
/*  91 */ "z",
/*  92 */ "braceleft",
/*  93 */ "bar",
/*  94 */ "braceright",
/*  95 */ "asciitilde",
/*  96 */ "exclamdown",
/*  97 */ "cent",
/*  98 */ "sterling",
/*  99 */ "fraction",
/* 100 */ "yen",
/* 101 */ "florin",
/* 102 */ "section",
/* 103 */ "currency",
/* 104 */ "quotesingle",
/* 105 */ "quotedblleft",
/* 106 */ "guillemotleft",
/* 107 */ "guilsinglleft",
/* 108 */ "guilsinglright",
/* 109 */ "fi",
/* 110 */ "fl",
/* 111 */ "endash",
/* 112 */ "dagger",
/* 113 */ "daggerdbl",
/* 114 */ "periodcentered",
/* 115 */ "paragraph",
/* 116 */ "bullet",
/* 117 */ "quotesinglbase",
/* 118 */ "quotedblbase",
/* 119 */ "quotedblright",
/* 120 */ "guillemotright",
/* 121 */ "ellipsis",
/* 122 */ "perthousand",
/* 123 */ "questiondown",
/* 124 */ "grave",
/* 125 */ "acute",
/* 126 */ "circumflex",
/* 127 */ "tilde",
/* 128 */ "macron",
/* 129 */ "breve",
/* 130 */ "dotaccent",
/* 131 */ "dieresis",
/* 132 */ "ring",
/* 133 */ "cedilla",
/* 134 */ "hungarumlaut",
/* 135 */ "ogonek",
/* 136 */ "caron",
/* 137 */ "emdash",
/* 138 */ "AE",
/* 139 */ "ordfeminine",
/* 140 */ "Lslash",
/* 141 */ "Oslash",
/* 142 */ "OE",
/* 143 */ "ordmasculine",
/* 144 */ "ae",
/* 145 */ "dotlessi",
/* 146 */ "lslash",
/* 147 */ "oslash",
/* 148 */ "oe",
/* 149 */ "germandbls",
/* 150 */ "onesuperior",
/* 151 */ "logicalnot",
/* 152 */ "mu",
/* 153 */ "trademark",
/* 154 */ "Eth",
/* 155 */ "onehalf",
/* 156 */ "plusminus",
/* 157 */ "Thorn",
/* 158 */ "onequarter",
/* 159 */ "divide",
/* 160 */ "brokenbar",
/* 161 */ "degree",
/* 162 */ "thorn",
/* 163 */ "threequarters",
/* 164 */ "twosuperior",
/* 165 */ "registered",
/* 166 */ "minus",
/* 167 */ "eth",
/* 168 */ "multiply",
/* 169 */ "threesuperior",
/* 170 */ "copyright",
/* 171 */ "Aacute",
/* 172 */ "Acircumflex",
/* 173 */ "Adieresis",
/* 174 */ "Agrave",
/* 175 */ "Aring",
/* 176 */ "Atilde",
/* 177 */ "Ccedilla",
/* 178 */ "Eacute",
/* 179 */ "Ecircumflex",
/* 180 */ "Edieresis",
/* 181 */ "Egrave",
/* 182 */ "Iacute",
/* 183 */ "Icircumflex",
/* 184 */ "Idieresis",
/* 185 */ "Igrave",
/* 186 */ "Ntilde",
/* 187 */ "Oacute",
/* 188 */ "Ocircumflex",
/* 189 */ "Odieresis",
/* 190 */ "Ograve",
/* 191 */ "Otilde",
/* 192 */ "Scaron",
/* 193 */ "Uacute",
/* 194 */ "Ucircumflex",
/* 195 */ "Udieresis",
/* 196 */ "Ugrave",
/* 197 */ "Yacute",
/* 198 */ "Ydieresis",
/* 199 */ "Zcaron",
/* 200 */ "aacute",
/* 201 */ "acircumflex",
/* 202 */ "adieresis",
/* 203 */ "agrave",
/* 204 */ "aring",
/* 205 */ "atilde",
/* 206 */ "ccedilla",
/* 207 */ "eacute",
/* 208 */ "ecircumflex",
/* 209 */ "edieresis",
/* 210 */ "egrave",
/* 211 */ "iacute",
/* 212 */ "icircumflex",
/* 213 */ "idieresis",
/* 214 */ "igrave",
/* 215 */ "ntilde",
/* 216 */ "oacute",
/* 217 */ "ocircumflex",
/* 218 */ "odieresis",
/* 219 */ "ograve",
/* 220 */ "otilde",
/* 221 */ "scaron",
/* 222 */ "uacute",
/* 223 */ "ucircumflex",
/* 224 */ "udieresis",
/* 225 */ "ugrave",
/* 226 */ "yacute",
/* 227 */ "ydieresis",
/* 228 */ "zcaron",
/* 229 */ "exclamsmall",
/* 230 */ "Hungarumlautsmall",
/* 231 */ "dollaroldstyle",
/* 232 */ "dollarsuperior",
/* 233 */ "ampersandsmall",
/* 234 */ "Acutesmall",
/* 235 */ "parenleftsuperior",
/* 236 */ "parenrightsuperior",
/* 237 */ "twodotenleader",
/* 238 */ "onedotenleader",
/* 239 */ "zerooldstyle",
/* 240 */ "oneoldstyle",
/* 241 */ "twooldstyle",
/* 242 */ "threeoldstyle",
/* 243 */ "fouroldstyle",
/* 244 */ "fiveoldstyle",
/* 245 */ "sixoldstyle",
/* 246 */ "sevenoldstyle",
/* 247 */ "eightoldstyle",
/* 248 */ "nineoldstyle",
/* 249 */ "commasuperior",
/* 250 */ "threequartersemdash",
/* 251 */ "periodsuperior",
/* 252 */ "questionsmall",
/* 253 */ "asuperior",
/* 254 */ "bsuperior",
/* 255 */ "centsuperior",
/* 256 */ "dsuperior",
/* 257 */ "esuperior",
/* 258 */ "isuperior",
/* 259 */ "lsuperior",
/* 260 */ "msuperior",
/* 261 */ "nsuperior",
/* 262 */ "osuperior",
/* 263 */ "rsuperior",
/* 264 */ "ssuperior",
/* 265 */ "tsuperior",
/* 266 */ "ff",
/* 267 */ "ffi",
/* 268 */ "ffl",
/* 269 */ "parenleftinferior",
/* 270 */ "parenrightinferior",
/* 271 */ "Circumflexsmall",
/* 272 */ "hyphensuperior",
/* 273 */ "Gravesmall",
/* 274 */ "Asmall",
/* 275 */ "Bsmall",
/* 276 */ "Csmall",
/* 277 */ "Dsmall",
/* 278 */ "Esmall",
/* 279 */ "Fsmall",
/* 280 */ "Gsmall",
/* 281 */ "Hsmall",
/* 282 */ "Ismall",
/* 283 */ "Jsmall",
/* 284 */ "Ksmall",
/* 285 */ "Lsmall",
/* 286 */ "Msmall",
/* 287 */ "Nsmall",
/* 288 */ "Osmall",
/* 289 */ "Psmall",
/* 290 */ "Qsmall",
/* 291 */ "Rsmall",
/* 292 */ "Ssmall",
/* 293 */ "Tsmall",
/* 294 */ "Usmall",
/* 295 */ "Vsmall",
/* 296 */ "Wsmall",
/* 297 */ "Xsmall",
/* 298 */ "Ysmall",
/* 299 */ "Zsmall",
/* 300 */ "colonmonetary",
/* 301 */ "onefitted",
/* 302 */ "rupiah",
/* 303 */ "Tildesmall",
/* 304 */ "exclamdownsmall",
/* 305 */ "centoldstyle",
/* 306 */ "Lslashsmall",
/* 307 */ "Scaronsmall",
/* 308 */ "Zcaronsmall",
/* 309 */ "Dieresissmall",
/* 310 */ "Brevesmall",
/* 311 */ "Caronsmall",
/* 312 */ "Dotaccentsmall",
/* 313 */ "Macronsmall",
/* 314 */ "figuredash",
/* 315 */ "hypheninferior",
/* 316 */ "Ogoneksmall",
/* 317 */ "Ringsmall",
/* 318 */ "Cedillasmall",
/* 319 */ "questiondownsmall",
/* 320 */ "oneeighth",
/* 321 */ "threeeighths",
/* 322 */ "fiveeighths",
/* 323 */ "seveneighths",
/* 324 */ "onethird",
/* 325 */ "twothirds",
/* 326 */ "zerosuperior",
/* 327 */ "foursuperior",
/* 328 */ "fivesuperior",
/* 329 */ "sixsuperior",
/* 330 */ "sevensuperior",
/* 331 */ "eightsuperior",
/* 332 */ "ninesuperior",
/* 333 */ "zeroinferior",
/* 334 */ "oneinferior",
/* 335 */ "twoinferior",
/* 336 */ "threeinferior",
/* 337 */ "fourinferior",
/* 338 */ "fiveinferior",
/* 339 */ "sixinferior",
/* 340 */ "seveninferior",
/* 341 */ "eightinferior",
/* 342 */ "nineinferior",
/* 343 */ "centinferior",
/* 344 */ "dollarinferior",
/* 345 */ "periodinferior",
/* 346 */ "commainferior",
/* 347 */ "Agravesmall",
/* 348 */ "Aacutesmall",
/* 349 */ "Acircumflexsmall",
/* 350 */ "Atildesmall",
/* 351 */ "Adieresissmall",
/* 352 */ "Aringsmall",
/* 353 */ "AEsmall",
/* 354 */ "Ccedillasmall",
/* 355 */ "Egravesmall",
/* 356 */ "Eacutesmall",
/* 357 */ "Ecircumflexsmall",
/* 358 */ "Edieresissmall",
/* 359 */ "Igravesmall",
/* 360 */ "Iacutesmall",
/* 361 */ "Icircumflexsmall",
/* 362 */ "Idieresissmall",
/* 363 */ "Ethsmall",
/* 364 */ "Ntildesmall",
/* 365 */ "Ogravesmall",
/* 366 */ "Oacutesmall",
/* 367 */ "Ocircumflexsmall",
/* 368 */ "Otildesmall",
/* 369 */ "Odieresissmall",
/* 370 */ "OEsmall",
/* 371 */ "Oslashsmall",
/* 372 */ "Ugravesmall",
/* 373 */ "Uacutesmall",
/* 374 */ "Ucircumflexsmall",
/* 375 */ "Udieresissmall",
/* 376 */ "Yacutesmall",
/* 377 */ "Thornsmall",
/* 378 */ "Ydieresissmall",
/* 379 */ "001.000",
/* 380 */ "001.001",
/* 381 */ "001.002",
/* 382 */ "001.003",
/* 383 */ "Black",
/* 384 */ "Bold",
/* 385 */ "Book",
/* 386 */ "Light",
/* 387 */ "Medium",
/* 388 */ "Regular",
/* 389 */ "Roman",
/* 390 */ "Semibold"
];

// char -> SID
static STANDARD_ENCODING: [SID; 256] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 0, 111, 112, 113, 114, 0, 115, 116, 117, 118, 119, 120, 121, 122, 0, 123, 0, 124, 125, 126, 127, 128, 129, 130, 131, 0, 132, 133, 0, 134, 135, 136, 137, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 138, 0, 139, 0, 0, 0, 0, 140, 141, 142, 143, 0, 0, 0, 0, 0, 144, 0, 0, 0, 145, 0, 0, 146, 147, 148, 149, 0, 0, 0, 0];

static EXPERT_ENCODING: [SID; 256] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 229, 230, 0, 231, 232, 233, 234, 235, 236, 237, 238, 13, 14, 15, 99, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 27, 28, 249, 250, 251, 252, 0, 253, 254, 255, 256, 257, 0, 0, 0, 258, 0, 0, 259, 260, 261, 262, 0, 0, 263, 264, 265, 0, 266, 109, 110, 267, 268, 269, 0, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283, 284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 303, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 304, 305, 306, 0, 0, 307, 308, 309, 310, 311, 0, 312, 0, 0, 313, 0, 0, 314, 315, 0, 0, 316, 317, 318, 0, 0, 0, 158, 155, 163, 319, 320, 321, 322, 323, 324, 325, 0, 0, 326, 150, 164, 169, 327, 328, 329, 330, 331, 332, 333, 334, 335, 336, 337, 338, 339, 340, 341, 342, 343, 344, 345, 346, 347, 348, 349, 350, 351, 352, 353, 354, 355, 356, 357, 358, 359, 360, 361, 362, 363, 364, 365, 366, 367, 368, 369, 370, 371, 372, 373, 374, 375, 376, 377, 378];
