use err::*;

use std;
use std::io::Write;
use std::vec::Vec;
use std::str::from_utf8;
use std::fmt::{Display, Formatter};
use std::collections::HashMap;
use file::File;
use object::{PlainRef, Resolve};

pub type Dictionary = HashMap<String, Primitive>;

/*
pub struct Stream<'a> {
    info: Dictionary,
    data: &'a [u8]
}
*/

#[derive(Clone, Debug)]
pub enum Primitive {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (Vec<u8>),
    //Stream (Stream),
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
            p => wrong_primitive!(Integer, p)
        }
    }
    pub fn as_reference(&self) -> Result<PlainRef> {
        match *self {
            Primitive::Reference(id) => Ok(id),
            p => wrong_primitive!(Reference, p)
        }
    }
    pub fn as_array(&self, resolve: &Resolve) -> Result<&[Primitive]> {
        match *self {
            Primitive::Array(ref v) => Ok(v),
            Primitive::Reference(id) => resolve(id)?.as_array(resolve),
            p => wrong_primitive!(Array, p)
        }
    }
    pub fn as_dictionary(&self, resolve: &Resolve) -> Result<&Dictionary> {
        match *self {
            Primitive::Dictionary(ref dict) => Ok(dict),
            Primitive::Reference(id) => resolve(id)?.as_dictionary(reader),
            p => wrong_primitive!(Dictionary, p)
        }
    }
    pub fn as_name(&self) -> Result<&str> {
        match *self {
            Primitive::Name(ref name) => Ok(name as &str),
            p => wrong_primitive!(Name, p)
        }
    }
    pub fn as_string(&self) -> Result<&[u8]> {
        match *self {
            Primitive::String(ref data) => Ok(data),
            p => wrong_primitive!(String, p)
        }
    }
/*
    pub fn as_stream(&self, reader: &File) -> Result<&Stream> {
        match *self {
            Primitive::Stream(ref s) => Ok(s),
            Primitive::Reference(id) => reader.dereference(&id)?.as_stream(reader),
            p => wrong_primitive!(Stream, p)
        }
    }
    */
}
