use err::*;

use std::collections::hash_map;
use std::{str, fmt, io};
use std::ops::{Index, Range};
use object::{PlainRef, Resolve, Object};
use chrono::{DateTime, FixedOffset};



#[derive(Clone, Debug)]
pub enum Primitive {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (PdfString),
    Stream (Stream),
    Dictionary (Dictionary),
    Array (Vec<Primitive>),
    Reference (PlainRef),
    Name (String),
}

/// Primitive Dictionary type.
#[derive(Default, Clone)]
pub struct Dictionary {
    dict: hash_map::HashMap<String, Primitive>
}
impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary { dict: hash_map::HashMap::new() }
    }
    pub fn get(&self, key: &str) -> Option<&Primitive> {
        self.dict.get(key)
    }
    pub fn insert(&mut self, key: String, val: Primitive) -> Option<Primitive> {
        self.dict.insert(key, val)
    }
    pub fn iter(&self) -> hash_map::Iter<String, Primitive> {
        self.dict.iter()
    }
    pub fn remove(&mut self, key: &str) -> Option<Primitive> {
        self.dict.remove(key)
    }
}
impl fmt::Debug for Dictionary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{{")?;
        for (k, v) in self {
            writeln!(f, "{:>15}: {:?}", k, v)?;
        }
        write!(f, "}}")
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
    type IntoIter = hash_map::Iter<'a, String, Primitive>;
    fn into_iter(self) -> Self::IntoIter {
        (&self.dict).into_iter()
    }
}
/// Primitive Stream type.
#[derive(Clone, Debug)]
pub struct Stream {
    pub info: Dictionary,
    pub data: Vec<u8>,
}
impl Object for Stream {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        writeln!(out, "<<")?;
        for (k, v) in &self.info {
            write!(out, "  {} ", k)?;
            v.serialize(out)?;
            writeln!(out, "")?;
        }
        writeln!(out, ">>")?;
        
        writeln!(out, "stream")?;
        out.write_all(&self.data)?;
        writeln!(out, "\nendstream")
    }
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        p.to_stream(resolve)
    }
}



macro_rules! unexpected_primitive {
    ($expected:ident, $found:expr) => (
        Err(ErrorKind::UnexpectedPrimitive {
            expected: stringify!($expected),
            found: $found
        }.into())
    )
}

/// Primitive String type.
#[derive(Clone)]
pub struct PdfString {
    data: Vec<u8>,
}
impl fmt::Debug for PdfString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "b\"")?;
        for &b in &self.data {
            match b {
                b'"' => write!(f, "\\\"")?,
                b' ' ... b'~' => write!(f, "{}", b as char)?,
                o @ 0 ... 7  => write!(f, "\\{}", o)?,
                x => write!(f, "\\x{:02x}", x)?
            }
        }
        Ok(())
    }
}
impl Object for PdfString {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
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
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        match p {
            Primitive::String (string) => Ok(string),
            _ => unexpected_primitive!(String, p.get_debug_name()),
        }
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
    pub fn as_str(&self) -> Result<&str> {
        Ok(str::from_utf8(&self.data)?)
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
    pub fn into_string(self) -> Result<String> {
        Ok(String::from_utf8(self.data)?)
    }
}


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
    pub fn to_reference(self) -> Result<PlainRef> {
        match self {
            Primitive::Reference(id) => Ok(id),
            p => unexpected_primitive!(Reference, p.get_debug_name())
        }
    }
    pub fn to_array(self, r: &Resolve) -> Result<Vec<Primitive>> {
        match self {
            Primitive::Array(v) => Ok(v),
            Primitive::Reference(id) => r.resolve(id)?.to_array(r),
            p => unexpected_primitive!(Array, p.get_debug_name())
        }
    }
    pub fn to_dictionary(self, r: &Resolve) -> Result<Dictionary> {
        match self {
            Primitive::Dictionary(dict) => Ok(dict),
            Primitive::Reference(id) => r.resolve(id)?.to_dictionary(r),
            p => unexpected_primitive!(Dictionary, p.get_debug_name())
        }
    }
    pub fn to_name(self) -> Result<String> {
        match self {
            Primitive::Name(name) => Ok(name),
            p => unexpected_primitive!(Name, p.get_debug_name())
        }
    }
    pub fn to_string(self) -> Result<PdfString> {
        match self {
            Primitive::String(data) => Ok(data),
            p => unexpected_primitive!(String, p.get_debug_name())
        }
    }
    pub fn to_stream(self, r: &Resolve) -> Result<Stream> {
        match self {
            Primitive::Stream (s) => Ok(s),
            Primitive::Reference (id) => r.resolve(id)?.to_stream(r),
            p => unexpected_primitive!(Stream, p.get_debug_name())
        }
    }
}


impl<T: Object> Object for Option<T> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        // TODO: the Option here is most often or always about whether the entry exists in a
        // dictionary. Hence it should probably be more up to the Dictionary impl of serialize, to
        // handle Options. 
        unimplemented!();
    }
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        Ok(
        match p {
            Primitive::Null => None,
            p => Some(T::from_primitive(p, r)?)
        }
        )
    }
}

fn parse_or<T: str::FromStr + Clone>(buffer: &str, range: Range<usize>, default: T) -> T {
    buffer.get(range)
        .map(|s| str::parse::<T>(s).unwrap_or(default.clone()))
        .unwrap_or(default)
}

impl Object for DateTime<FixedOffset> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        // TODO: smal/avg amount of work.
        unimplemented!();
    }
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
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

