use err::*;

use std::vec::Vec;
use std::collections::HashMap;
use object::{PlainRef, Resolve, FromPrimitive};

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
            expected: stringify!(expected),
            found: "something else"
        }.into())
    )
}

impl Primitive {
    pub fn as_integer(&self) -> Result<i32> {
        match *self {
            Primitive::Integer(n) => Ok(n),
            ref p => wrong_primitive!(Integer, p)
        }
    }
    pub fn as_reference(&self) -> Result<PlainRef> {
        match *self {
            Primitive::Reference(id) => Ok(id),
            ref p => wrong_primitive!(Reference, p)
        }
    }
    pub fn as_array<'a>(&'a self, resolve: &Resolve<'a>) -> Result<&'a [Primitive]> {
        match *self {
            Primitive::Array(ref v) => Ok(v),
            Primitive::Reference(id) => resolve(id)?.as_array(resolve),
            ref p => wrong_primitive!(Array, p)
        }
    }
    pub fn as_dictionary<'a>(&'a self, resolve: &Resolve<'a>) -> Result<&'a Dictionary> {
        match *self {
            Primitive::Dictionary(ref dict) => Ok(dict),
            Primitive::Reference(id) => resolve(id)?.as_dictionary(resolve),
            ref p => wrong_primitive!(Dictionary, p)
        }
    }
    pub fn as_name(&self) -> Result<&str> {
        match *self {
            Primitive::Name(ref name) => Ok(name as &str),
            ref p => wrong_primitive!(Name, p)
        }
    }
    pub fn as_string(&self) -> Result<&[u8]> {
        match *self {
            Primitive::String(ref data) => Ok(data),
            ref p => wrong_primitive!(String, p)
        }
    }
    pub fn as_stream<'a>(&'a self, resolve: &Resolve<'a>) -> Result<&'a Stream> {
        match *self {
            Primitive::Stream (ref s) => Ok(s),
            Primitive::Reference (id) => resolve(id)?.as_stream(resolve),
            ref p => wrong_primitive!(Stream, p)
        }
    }
}



impl FromPrimitive for String {
    fn from_primitive(p: &Primitive, _: &Resolve) -> Result<Self> {
        Ok(p.as_name()?.to_owned())
    }
}

impl<T: FromPrimitive> FromPrimitive for Vec<T> {
    fn from_primitive(p: &Primitive, r: &Resolve) -> Result<Self> {
        Ok(p.as_array(r)?.iter().map(|p| T::from_primitive(p, r)).collect::<Result<Vec<T>>>()?)
    }
}


impl FromPrimitive for i32 {
    fn from_primitive(p: &Primitive, _: &Resolve) -> Result<Self> {
        p.as_integer()
    }
}


// FromPrimitive for inner values of Primitive variants - target for macro rules?
impl FromPrimitive for Dictionary {
    fn from_primitive(p: &Primitive, _: &Resolve) -> Result<Self> {
        match *p {
            Primitive::Dictionary (ref d) => Ok(d.clone()),
            _ => Err(ErrorKind::WrongObjectType { expected: "Dictionary", found: "something else"}.into()),
        }
    }
}
