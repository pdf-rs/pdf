use crate::error::*;
use crate::object::{PlainRef, Resolve, Object};

use std::collections::{btree_map, BTreeMap};
use std::{str, fmt, io};
use std::ops::{Index, Range};
use chrono::{DateTime, FixedOffset};
use std::ops::Deref;
use std::convert::TryInto;
use std::borrow::Cow;
use itertools::Itertools;

#[derive(Clone, Debug)]
pub enum Primitive {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (PdfString),
    Stream (PdfStream),
    Dictionary (Dictionary),
    Array (Vec<Primitive>),
    Reference (PlainRef),
    Name (String),
}

impl fmt::Display for Primitive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Primitive::Null => write!(f, "null"),
            Primitive::Integer(i) => i.fmt(f),
            Primitive::Number(n) => n.fmt(f),
            Primitive::Boolean(b) => b.fmt(f),
            Primitive::String(ref s) => write!(f, "{:?}", s),
            Primitive::Stream(_) => write!(f, "stream"),
            Primitive::Dictionary(ref d) => d.fmt(f),
            Primitive::Array(ref arr) => write!(f, "[{}]", arr.iter().format(", ")),
            Primitive::Reference(r) => write!(f, "@{}", r.id),
            Primitive::Name(ref s) => write!(f, "/{}", s)
        }
    }
}

/// Primitive Dictionary type.
#[derive(Default, Clone)]
pub struct Dictionary {
    dict: BTreeMap<String, Primitive>
}
impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary { dict: BTreeMap::new()}
    }
    pub fn len(&self) -> usize {
        self.dict.len()
    }
    pub fn get(&self, key: &str) -> Option<&Primitive> {
        self.dict.get(key)
    }
    pub fn insert(&mut self, key: String, val: Primitive) -> Option<Primitive> {
        self.dict.insert(key, val)
    }
    pub fn iter(&self) -> btree_map::Iter<String, Primitive> {
        self.dict.iter()
    }
    pub fn remove(&mut self, key: &str) -> Option<Primitive> {
        self.dict.remove(key)
    }
    /// like remove, but takes the name of the calling type and returns `PdfError::MissingEntry` if the entry is not found
    pub fn require(&mut self, typ: &'static str, key: &str) -> Result<Primitive> {
        self.remove(key).ok_or(
            PdfError::MissingEntry {
                typ,
                field: key.into()
            }
        )
    }
    /// assert that the given key/value pair is in the dictionary (`required=true`),
    /// or the key is not present at all (`required=false`)
    pub fn expect(&self, typ: &'static str, key: &str, value: &str, required: bool) -> Result<()> {
        match self.dict.get(key) {
            Some(ty) => {
                let ty = ty.as_name()?;
                if ty != value {
                    Err(PdfError::KeyValueMismatch {
                        key: key.into(),
                        value: value.into(),
                        found: ty.into()
                    })
                } else {
                    Ok(())
                }
            },
            None if required => Err(PdfError::MissingEntry { typ, field: key.into() }),
            None => Ok(())
        }
    }
}
impl Deref for Dictionary {
    type Target = BTreeMap<String, Primitive>;
    fn deref(&self) -> &BTreeMap<String, Primitive> {
        &self.dict
    }
}
impl fmt::Debug for Dictionary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{{")?;
        for (k, v) in self {
            writeln!(f, "{:>15}: {}", k, v)?;
        }
        write!(f, "}}")
    }
}
impl fmt::Display for Dictionary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<{}>", self.iter().format_with(", ", |(k, v), f| f(&format_args!("{}={}", k, v))))
    }
}
impl<'a> Index<&'a str> for Dictionary {
    type Output = Primitive;
    fn index(&self, idx: &'a str) -> &Primitive {
        self.dict.index(idx)
    }
}
impl<'a> IntoIterator for &'a Dictionary {
    type Item = (&'a String, &'a Primitive);
    type IntoIter = btree_map::Iter<'a, String, Primitive>;
    fn into_iter(self) -> Self::IntoIter {
        (&self.dict).into_iter()
    }
}

/// Primitive Stream (as opposed to the higher-level `Stream`)
#[derive(Clone, Debug)]
pub struct PdfStream {
    pub info: Dictionary,
    pub data: Vec<u8>,
}
impl Object for PdfStream {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()>  {
        writeln!(out, "<<")?;
        for (k, v) in &self.info {
            write!(out, "  {} ", k)?;
            v.serialize(out)?;
            writeln!(out, "")?;
        }
        writeln!(out, ">>")?;
        
        writeln!(out, "stream")?;
        out.write_all(&self.data)?;
        writeln!(out, "\nendstream")?;
        Ok(())
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Stream (stream) => Ok(stream),
            Primitive::Reference (r) => PdfStream::from_primitive(resolve.resolve(r)?, resolve),
            p => Err(PdfError::UnexpectedPrimitive {expected: "Stream", found: p.get_debug_name()})
        }
    }
}



macro_rules! unexpected_primitive {
    ($expected:ident, $found:expr) => (
        Err(PdfError::UnexpectedPrimitive {
            expected: stringify!($expected),
            found: $found
        })
    )
}

/// Primitive String type.
#[derive(Clone)]
pub struct PdfString {
    pub data: Vec<u8>,
}
impl fmt::Debug for PdfString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "\"")?;
        for &b in &self.data {
            match b {
                b'"' => write!(f, "\\\"")?,
                b' ' ..= b'~' => write!(f, "{}", b as char)?,
                o @ 0 ..= 7  => write!(f, "\\{}", o)?,
                x => write!(f, "\\x{:02x}", x)?
            }
        }
        write!(f, "\"")
    }
}
impl Object for PdfString {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, r"\")?;
        for &b in &self.data {
            match b {
                b'\\' | b'(' | b')' => write!(out, r"\")?,
                c if c > b'~' => panic!("only ASCII"),
                _ => ()
            }
            write!(out, "{}", b)?;
        }
        Ok(())
    }
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::String (string) => Ok(string),
            Primitive::Reference(id) => Ok(r.resolve(id)?.to_string()?),
            _ => unexpected_primitive!(String, p.get_debug_name()),
        }
    }
}
impl AsRef<[u8]> for PdfString {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PdfString {
    pub fn new(data: Vec<u8>) -> PdfString {
        PdfString {
            data: data
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn as_str(&self) -> Result<Cow<str>> {
        if self.data.starts_with(&[0xfe, 0xff]) {
            // FIXME: avoid extra allocation
            let utf16: Vec<u16> = self.data[2..].chunks(2).map(|c| (c[0] as u16) << 8 | c[1] as u16).collect();
            Ok(Cow::Owned(String::from_utf16_lossy(&utf16)))
        } else {
            Ok(String::from_utf8_lossy(&self.data))
        }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
    pub fn into_string(self) -> Result<String> {
        Ok(self.as_str()?.into_owned())
    }
}


// TODO:
// Noticed some inconsistency here.. I think to_* and as_* should not take Resolve, and not accept
// Reference. Only from_primitive() for the respective type resolves References.
impl Primitive {
    /// For debugging / error messages: get the name of the variant
    pub fn get_debug_name(&self) -> &'static str {
        match *self {
            Primitive::Null => "Null",
            Primitive::Integer (..) => "Integer",
            Primitive::Number (..) => "Number",
            Primitive::Boolean (..) => "Boolean",
            Primitive::String (..) => "String",
            Primitive::Stream (..) => "Stream",
            Primitive::Dictionary (..) => "Dictionary",
            Primitive::Array (..) => "Array",
            Primitive::Reference (..) => "Reference",
            Primitive::Name (..) => "Name",
        }
    }
    pub fn as_integer(&self) -> Result<i32> {
        match *self {
            Primitive::Integer(n) => Ok(n),
            ref p => unexpected_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_u32(&self) -> Result<u32> {
        match *self {
            Primitive::Integer(n) if n >= 0 => Ok(n as u32),
            Primitive::Integer(n) => bail!("negative integer"),
            ref p => unexpected_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_number(&self) -> Result<f32> {
        match *self {
            Primitive::Integer(n) => Ok(n as f32),
            Primitive::Number(f) => Ok(f),
            ref p => unexpected_primitive!(Number, p.get_debug_name())
        }
    }
    pub fn as_bool(&self) -> Result<bool> {
        match *self {
            Primitive::Boolean (b) => Ok(b),
            ref p => unexpected_primitive!(Number, p.get_debug_name())
        }
    }
    pub fn as_name(&self) -> Result<&str> {
        match self {
            Primitive::Name(ref name) => Ok(name.as_str()),
            p => unexpected_primitive!(Name, p.get_debug_name())
        }
    }
    pub fn as_string(&self) -> Result<&PdfString> {
        match self {
            Primitive::String(ref data) => Ok(data),
            p => unexpected_primitive!(String, p.get_debug_name())
        }
    }
    pub fn as_str(&self) -> Option<Cow<str>> {
        self.as_string().ok().and_then(|s| s.as_str().ok())
    }
    /// Does not accept a Reference
    pub fn as_array(&self) -> Result<&[Primitive]> {
        match self {
            Primitive::Array(ref v) => Ok(v),
            p => unexpected_primitive!(Array, p.get_debug_name())
        }
    }
    pub fn to_reference(self) -> Result<PlainRef> {
        match self {
            Primitive::Reference(id) => Ok(id),
            p => unexpected_primitive!(Reference, p.get_debug_name())
        }
    }
    /// Does accept a Reference
    pub fn to_array(self, r: &impl Resolve) -> Result<Vec<Primitive>> {
        match self {
            Primitive::Array(v) => Ok(v),
            Primitive::Reference(id) => r.resolve(id)?.to_array(r),
            p => unexpected_primitive!(Array, p.get_debug_name())
        }
    }
    pub fn to_dictionary(self, r: &impl Resolve) -> Result<Dictionary> {
        match self {
            Primitive::Dictionary(dict) => Ok(dict),
            Primitive::Reference(id) => r.resolve(id)?.to_dictionary(r),
            p => unexpected_primitive!(Dictionary, p.get_debug_name())
        }
    }
    /// Doesn't accept a Reference
    pub fn to_name(self) -> Result<String> {
        match self {
            Primitive::Name(name) => Ok(name),
            p => unexpected_primitive!(Name, p.get_debug_name())
        }
    }
    /// Doesn't accept a Reference
    pub fn to_string(self) -> Result<PdfString> {
        match self {
            Primitive::String(data) => Ok(data),
            p => unexpected_primitive!(String, p.get_debug_name())
        }
    }
    /// Doesn't accept a Reference
    pub fn to_stream(self, _r: &impl Resolve) -> Result<PdfStream> {
        match self {
            Primitive::Stream (s) => Ok(s),
            // Primitive::Reference (id) => r.resolve(id)?.to_stream(r),
            p => unexpected_primitive!(Stream, p.get_debug_name())
        }
    }
}

impl From<i32> for Primitive {
    fn from(x: i32) -> Primitive {
        Primitive::Integer(x)
    }
}
impl From<f32> for Primitive {
    fn from(x: f32) -> Primitive {
        Primitive::Number(x)
    }
}
impl From<bool> for Primitive {
    fn from(x: bool) -> Primitive {
        Primitive::Boolean(x)
    }
}
impl From<PdfString> for Primitive {
    fn from(x: PdfString) -> Primitive {
        Primitive::String (x)
    }
}
impl From<PdfStream> for Primitive {
    fn from(x: PdfStream) -> Primitive {
        Primitive::Stream (x)
    }
}
impl From<Dictionary> for Primitive {
    fn from(x: Dictionary) -> Primitive {
        Primitive::Dictionary (x)
    }
}
impl From<Vec<Primitive>> for Primitive {
    fn from(x: Vec<Primitive>) -> Primitive {
        Primitive::Array (x)
    }
}

impl From<PlainRef> for Primitive {
    fn from(x: PlainRef) -> Primitive {
        Primitive::Reference (x)
    }
}
impl From<String> for Primitive {
    fn from(x: String) -> Primitive {
        Primitive::Name (x)
    }
}
impl<'a> TryInto<f32> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<f32> {
        self.as_number()
    }
}
impl<'a> TryInto<i32> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<i32> {
        self.as_integer()
    }
}
impl<'a> TryInto<&'a [Primitive]> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<&'a [Primitive]> {
        self.as_array()
    }
}
impl<'a> TryInto<&'a [u8]> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<&'a [u8]> {
        match self {
            Primitive::Name(ref s) => Ok(s.as_bytes()),
            Primitive::String(ref s) => Ok(s.as_bytes()),
            ref p => Err(PdfError::UnexpectedPrimitive {
                expected: "Name or String",
                found: p.get_debug_name()
            })
        }
    }
}
impl<'a> TryInto<Cow<'a, str>> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<Cow<'a, str>> {
        match self {
            Primitive::Name(ref s) => Ok(Cow::Borrowed(&*s)),
            Primitive::String(ref s) => Ok(s.as_str()?),
            ref p => Err(PdfError::UnexpectedPrimitive {
                expected: "Name or String",
                found: p.get_debug_name()
            })
        }
    }
}
impl<'a> TryInto<String> for &'a Primitive {
    type Error = PdfError;
    fn try_into(self) -> Result<String> {
        match self {
            Primitive::Name(ref s) => Ok(s.clone()),
            Primitive::String(ref s) => Ok(s.as_str()?.into_owned()),
            ref p => Err(PdfError::UnexpectedPrimitive {
                expected: "Name or String",
                found: p.get_debug_name()
            })
        }
    }
}

fn parse_or<T: str::FromStr + Clone>(buffer: &str, range: Range<usize>, default: T) -> T {
    buffer.get(range)
        .map(|s| str::parse::<T>(s).unwrap_or(default.clone()))
        .unwrap_or(default)
}

impl Object for DateTime<FixedOffset> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {
        // TODO: smal/avg amount of work.
        unimplemented!();
    }
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        use chrono::{NaiveDateTime, NaiveDate, NaiveTime};
        match p {
            Primitive::String (PdfString {data}) => {
                let s = str::from_utf8(&data)?;
                let len = s.len();
                if len > 2 && &s[0..2] == "D:" {

                    let year = match s.get(2..6) {
                        Some(year) => {
                            str::parse::<i32>(year)?
                        }
                        None => bail!("Missing obligatory year in date")
                    };
                    let month = parse_or(s, 6..8, 1);
                    let day = parse_or(s, 8..10, 1);
                    let hour = parse_or(s, 10..12, 0);
                    let minute = parse_or(s, 12..14, 0);
                    let second = parse_or(s, 14..16, 0);
                    let tz_hour = parse_or(s, 16..18, 0);
                    let tz_minute = parse_or(s, 19..21, 0);
                    let tz = FixedOffset::east(tz_hour * 60 + tz_minute);

                    Ok(DateTime::from_utc(
                            NaiveDateTime::new(NaiveDate::from_ymd(year, month, day),
                                               NaiveTime::from_hms(hour, minute, second)),
                          tz
                      ))

                } else {
                    bail!("Failed parsing date");
                }
            }
            _ => unexpected_primitive!(String, p.get_debug_name()),
        }
    }
}

