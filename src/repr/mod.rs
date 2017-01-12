//! Runtime representation of a PDF file.

use std::vec::Vec;
use err::*;

/// Runtime representation of a PDF file.
pub struct PDF {
    // Thoughts...
    // xref tables are kind of interleaved with other things..
}
impl PDF {
    pub fn new() -> PDF {
        PDF {
        }
    }
}


/* Cross-reference table */
pub struct XrefTable {
    pub first_id: u32,
    pub entries: Vec<XrefEntry>,
}

#[derive(Copy,Clone)]
pub enum XrefEntry {
    Free{next_obj_nr: u32, gen_nr: u16},
    InUse{pos: usize, gen_nr: u16},
}

impl XrefTable {
    pub fn new(first_id: u32) -> XrefTable {
        XrefTable {
            first_id: first_id,
            entries: Vec::new(),
        }
    }
    pub fn add_free_entry(&mut self, next_obj_nr: u32, gen_nr: u16) {
        self.entries.push(XrefEntry::Free{next_obj_nr: next_obj_nr, gen_nr: gen_nr});
    }
    pub fn add_inuse_entry(&mut self, pos: usize, gen_nr: u16) {
        self.entries.push(XrefEntry::InUse{pos: pos, gen_nr: gen_nr});
    }
}

/* Objects */
pub struct IndirectObject {
    pub obj_nr: i32,
    pub gen_nr: i32,
    pub object: Object,
}

#[derive(Clone)]
pub enum Object {
    Integer (i32),
    RealNumber (f32),
    Boolean (bool),
    String(StringType, String),
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
}

// TODO should this also be used for writing objects to file? - or should that be Debug or Display
// trait?
impl ToString for Object {
    fn to_string(&self) -> String {
        match self {
            &Object::Integer(n) => n.to_string(),
            &Object::RealNumber(n) => n.to_string(),
            &Object::Boolean(b) => b.to_string(),
            &Object::String(ref t, ref s) => {
                match t {
                    &StringType::HEX => "HexString(".to_string() + s.as_str() +")",
                    &StringType::UTF8 => "UtfString(".to_string() + s.as_str() + ")",
                }
            },
            &Object::Stream{filters: _, dictionary: _, ref content} => "Stream(".to_string() + content.as_str() + ")",
            &Object::Dictionary(_) => "Object::Dictionary".to_string(),
            &Object::Array(_) => "Object::Array".to_string(),
            &Object::Reference{obj_nr: _, gen_nr: _} => "Object::Reference".to_string(),
            &Object::Name (_) => "Object::Name".to_string(),
            &Object::Null => "Object::Null".to_string(),
        }
    }
}

/*
#[derive(Clone)]
pub struct Name(pub String); // Is technically an object but I keep it outside for now
// TODO Name could be an enum if Names are really a known finite set. Easy comparision
*/

#[derive(Clone)]
pub enum StringType {
    HEX, UTF8
}
#[derive(Clone)]
pub enum Filter {
    ASCIIHexDecode,
    ASCII85Decode,
    // etc...
}
