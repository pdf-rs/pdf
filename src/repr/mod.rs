//! Runtime representation of a PDF file.

mod xref;

pub use self::xref::*;

use std::vec::Vec;
use err::*;
use std;
use std::str::from_utf8;
use std::fmt::{Display, Debug, Formatter};


/* Objects */
pub struct IndirectObject {
    pub obj_nr: i32,
    pub gen_nr: i32,
    pub object: Object,
}

#[derive(Clone, Debug)]
pub enum Object {
    Integer (i32),
    RealNumber (f32),
    Boolean (bool),
    String (Vec<u8>),
    Stream {filters: Vec<String>, dictionary: Vec<(String, Object)>, content: String},
    Dictionary (Vec<(String, Object)>),
    Array (Vec<Object>),
    Reference {obj_nr: i32, gen_nr: i32},
    Name (String),
    Null,
}

impl Object {
    /// `self` must be an `Object::Dictionary`.
    pub fn dict_get<'a>(&'a self, key: String) -> Result<&'a Object> {
        match self {
            &Object::Dictionary(ref dictionary) => {
                for &(ref name, ref object) in dictionary {
                    if key == *name {
                        return Ok(object);
                    }
                }
                Err(ErrorKind::NotFound {word: key}.into())
            },
            _ => {
                Err(ErrorKind::WrongObjectType.into())
            }
        }
    }
    pub fn unwrap_integer(&self) -> Result<i32> {
        match self {
            &Object::Integer(n) => Ok(n),
            _ => {
                // Err(ErrorKind::WrongObjectType.into()).chain_err(|| ErrorKind::ExpectedType {expected: "Reference"})
                Err(ErrorKind::WrongObjectType.into())
            }
        }
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            &Object::Integer(n) => write!(f, "{}", n),
            &Object::RealNumber(n) => write!(f, "{}", n),
            &Object::Boolean(b) => write!(f, "{}", if b {"true"} else {"false"}),
            &Object::String (ref s) => {
                let decoded = from_utf8(s);
                match decoded {
                    Ok(decoded) => write!(f, "({})", decoded),
                    Err(_) => {
                        // Write out bytes as numbers.
                        write!(f, "encoded(")?;
                        for c in s {
                            write!(f, "{},", c)?;
                        }
                        write!(f, ")")
                    }
                }
            },
            &Object::Stream{filters: _, dictionary: _, ref content} => write!(f, "stream\n{}\nendstream\n", content.as_str()),
            &Object::Dictionary(ref d) => {
                write!(f, "<< ")?;
                for e in d {
                    write!(f, "/{} {}", e.0, e.1)?;
                }
                write!(f, ">>\n")
            },
            &Object::Array(ref a) => {
                write!(f, "[")?;
                for e in a {
                    write!(f, "{} ", e)?;
                }
                write!(f, "]")
            },
            &Object::Reference{obj_nr, gen_nr} => {
                write!(f, "{} {} R", obj_nr, gen_nr)
            },
            &Object::Name (ref name) => write!(f, "/{}", name),
            &Object::Null => write!(f, "Null"),
        }
    }
}

/*
#[derive(Clone)]
pub struct Name(pub String); // Is technically an object but I keep it outside for now
// TODO Name could be an enum if Names are really a known finite set. Easy comparision
*/

#[derive(Clone, Debug)]
pub enum StringType {
    HEX, UTF8
}
#[derive(Clone)]
pub enum Filter {
    ASCIIHexDecode,
    ASCII85Decode,
    // etc...
}
