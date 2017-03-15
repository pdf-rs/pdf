use err::*;

use std;
use std::io::Write;
use std::vec::Vec;
use std::str::from_utf8;
use std::fmt::{Display, Formatter};
use std::collections::HashMap;

/* Objects */
pub struct IndirectObject {
    pub id: ObjectId,
    pub object: Primitive,
}

pub type Dictionary = HashMap<String, Primitive>;

#[derive(Clone, Debug)]
pub enum Primitive {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (Vec<u8>),
    Stream (Stream),
    Dictionary (HashMap<String, Primitive>),
    Array (Vec<Primitive>),
    Reference (ObjectId),
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
            p => wrong_primitive!(Integer, p)
        }
    }
    pub fn as_reference(&self) -> Result<ObjectId> {
        match *self {
            Primitive::Reference(id) => Ok(id),
            p => wrong_primitive!(Reference, p)
        }
    }
    pub fn as_array(&self, reader: &Reader) -> Result<&[Primitive]> {
        match *self {
            Primitive::Array(ref v) => Ok(v),
            Primitive::Reference(id) => reader.dereference(&id)?.as_array(reader),
            p => wrong_primitive!(Array, p)
        }
    }
    pub fn as_dictionary(&self, reader: &Reader) -> Result<&Dictionary> {
        match *self {
            Primitive::Dictionary(ref dict) => Ok(dict),
            Primitive::Reference(id) => reader.dereference(&id)?.as_dictionary(reader),
            p => wrong_primitive!(Dictionary, p)
        }
    }

    pub fn as_stream(&self, reader: &Reader) -> Result<&Stream> {
        match *self {
            Primitive::Stream(ref s) => Ok(s),
            Primitive::Reference(id) => reader.dereference(&id)?.as_stream(reader),
            p => wrong_primitive!(Stream, p)
        }
    }
}

/// PDF stream object.
#[derive(Clone, Debug)]
pub struct Stream {
    pub dictionary: Dictionary,
    offset: usize,
    length: usize
}
impl Stream {
    pub fn data(reader: &Reader) -> &[u8] {
        &reader.data()[self.offset .. self.offset + self.length]
    }
}

/// Used to identify an object; corresponds to a PDF indirect reference.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId {
    pub obj_nr: u32,
    pub gen_nr: u16,
}
