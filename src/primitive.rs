use err::*;

use std::vec::Vec;
use std::collections::HashMap;
use object::{PlainRef, Resolve, FromPrimitive, FromDict, FromStream};

pub type Dictionary = HashMap<String, Primitive>;

#[derive(Clone, Debug)]
pub struct Stream {
    pub info: Dictionary,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum Primitive {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (Vec<u8>),
    Stream (Stream),
    Dictionary (Dictionary),
    Array (Vec<Primitive>),
    Reference (PlainRef),
    Name (String),
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
    pub fn as_string(self) -> Result<Vec<u8>> {
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
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        Ok(p.as_array(r)?
            .into_iter()
            .map(|p| T::from_primitive(p, r))
            .collect::<Result<Vec<T>>>()?
        )
    }
}


impl FromPrimitive for i32 {
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        p.as_integer()
    }
}


// FromPrimitive for inner values of Primitive variants - target for macro rules?
impl FromPrimitive for Dictionary {
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        match p {
            Primitive::Dictionary (d) => Ok(d),
            _ => Err(ErrorKind::WrongObjectType { expected: "Dictionary", found: "something else"}.into()),
        }
    }
}
