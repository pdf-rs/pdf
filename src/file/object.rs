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
pub enum Primitive<'a> {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (Vec<u8>),
    Stream (Stream<'a>),
    Dictionary (HashMap<String, Primitive<'a>>),
    Array (Vec<Primitive<'a>>),
    Reference (ObjectId),
    Name (String),
}


/// PDF stream object.
#[derive(Clone, Debug)]
pub struct Stream<'a> {
    pub dictionary: Dictionary,
    pub content: &'a[u8],
}

/// Used to identify an object; corresponds to a PDF indirect reference.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId {
    pub obj_nr: u32,
    pub gen_nr: u16,
}
