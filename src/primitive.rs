use err::*;

use std::vec::Vec;
use std::collections::HashMap;
use std::str;
use object::{PlainRef, Resolve, FromPrimitive, FromDict, FromStream};



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
pub type Dictionary = HashMap<String, Primitive>;

/// Primitive Stream type.
#[derive(Clone, Debug)]
pub struct Stream {
    pub info: Dictionary,
    pub data: Vec<u8>,
}

/// Primitive String type.
#[derive(Clone, Debug)]
pub struct PdfString {
    data: Vec<u8>,
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

macro_rules! wrong_primitive {
    ($expected:ident, $found:expr) => (
        Err(ErrorKind::WrongObjectType {
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
    pub fn as_integer(self) -> Result<i32> {
        match self {
            Primitive::Integer(n) => Ok(n),
            p => wrong_primitive!(Integer, p.get_debug_name())
        }
    }
    pub fn as_reference(self) -> Result<PlainRef> {
        match self {
            Primitive::Reference(id) => Ok(id),
            p => wrong_primitive!(Reference, p.get_debug_name())
        }
    }
    pub fn as_array(self, r: &Resolve) -> Result<Vec<Primitive>> {
        match self {
            Primitive::Array(v) => Ok(v),
            Primitive::Reference(id) => r.resolve(id)?.as_array(r),
            p => wrong_primitive!(Array, p.get_debug_name())
        }
    }
    pub fn as_dictionary(self, r: &Resolve) -> Result<Dictionary> {
        match self {
            Primitive::Dictionary(dict) => Ok(dict),
            Primitive::Reference(id) => r.resolve(id)?.as_dictionary(r),
            p => wrong_primitive!(Dictionary, p.get_debug_name())
        }
    }
    pub fn as_name(self) -> Result<String> {
        match self {
            Primitive::Name(name) => Ok(name),
            p => wrong_primitive!(Name, p.get_debug_name())
        }
    }
    pub fn as_string(self) -> Result<PdfString> {
        match self {
            Primitive::String(data) => Ok(data),
            p => wrong_primitive!(String, p.get_debug_name())
        }
    }
    pub fn as_stream(self, r: &Resolve) -> Result<Stream> {
        match self {
            Primitive::Stream (s) => Ok(s),
            Primitive::Reference (id) => r.resolve(id)?.as_stream(r),
            p => wrong_primitive!(Stream, p.get_debug_name())
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

impl FromPrimitive for PdfString {
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        match p {
            Primitive::String (string) => Ok(string),
            _ => wrong_primitive!(String, p.get_debug_name()),
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
