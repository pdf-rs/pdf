use err::*;

use std::collections::hash_map;
use std::{str, fmt, io};
use std::ops::{Index, IndexMut};
use object::{PlainRef, Resolve, FromPrimitive, Object};



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
        for (ref k, ref v) in self {
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
        out.write(&self.data)?;
        writeln!(out, "\nendstream")
    }
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

macro_rules! unexpected_primitive {
    ($expected:ident, $found:expr) => (
        Err(ErrorKind::UnexpectedPrimitive {
            expected: stringify!($expected),
            found: $found
        }.into())
    )
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
        match self {
            &Primitive::Integer(n) => Ok(n),
            p => unexpected_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_number(&self) -> Result<f32> {
        match self {
            &Primitive::Integer(n) => Ok(n as f32),
            &Primitive::Number(f) => Ok(f),
            p => unexpected_primitive!(Number, p.get_debug_name())
        }
    }
    pub fn as_reference(self) -> Result<PlainRef> {
        match self {
            Primitive::Reference(id) => Ok(id),
            p => unexpected_primitive!(Reference, p.get_debug_name())
        }
    }
    pub fn as_array(self, r: &Resolve) -> Result<Vec<Primitive>> {
        match self {
            Primitive::Array(v) => Ok(v),
            Primitive::Reference(id) => r.resolve(id)?.as_array(r),
            p => unexpected_primitive!(Array, p.get_debug_name())
        }
    }
    pub fn as_dictionary(self, r: &Resolve) -> Result<Dictionary> {
        match self {
            Primitive::Dictionary(dict) => Ok(dict),
            Primitive::Reference(id) => r.resolve(id)?.as_dictionary(r),
            p => unexpected_primitive!(Dictionary, p.get_debug_name())
        }
    }
    pub fn as_name(self) -> Result<String> {
        match self {
            Primitive::Name(name) => Ok(name),
            p => unexpected_primitive!(Name, p.get_debug_name())
        }
    }
    pub fn as_string(self) -> Result<PdfString> {
        match self {
            Primitive::String(data) => Ok(data),
            p => unexpected_primitive!(String, p.get_debug_name())
        }
    }
    pub fn as_stream(self, r: &Resolve) -> Result<Stream> {
        match self {
            Primitive::Stream (s) => Ok(s),
            Primitive::Reference (id) => r.resolve(id)?.as_stream(r),
            p => unexpected_primitive!(Stream, p.get_debug_name())
        }
    }
}



impl FromPrimitive for String {
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        Ok(p.as_name()?)
    }
}

impl<T: FromPrimitive> FromPrimitive for Vec<T> {
    /// Will try to convert `p` to `T` first, then try to convert `p` to Vec<T>
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        Ok(
        match p {
            Primitive::Array(_) => {
                p.as_array(r)?
                    .into_iter()
                    .map(|p| T::from_primitive(p, r))
                    .collect::<Result<Vec<T>>>()?
            }
            _ => vec![T::from_primitive(p, r)?]
        }
        )
    }
}

impl<T: FromPrimitive> FromPrimitive for Option<T> {
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        Ok(
        match p {
            Primitive::Null => None,
            p => Some(T::from_primitive(p, r)?)
        }
        )
    }
}

impl FromPrimitive for PdfString {
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        match p {
            Primitive::String (string) => Ok(string),
            _ => unexpected_primitive!(String, p.get_debug_name()),
        }
    }
}

impl FromPrimitive for i32 {
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        p.as_integer()
    }
}


// FromPrimitive for inner values of Primitive variants - target for macro rules?
impl FromPrimitive for Dictionary {
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        p.as_dictionary(r)
    }
}
