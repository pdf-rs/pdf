//! Runtime representation of a PDF file.

use std::io;
use std::vec::Vec;
use std::string;
use file_reader::lexer::Lexer;

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
    pub first_id: usize,
    pub entries: Vec<XrefEntry>,
}
pub enum XrefEntry {
    Free{obj_nr: usize, next_free: usize},
    InUse{pos: usize, gen_nr: usize},
}

impl XrefTable {
    pub fn new(first_id: usize) -> XrefTable {
        XrefTable {
            first_id: first_id,
            entries: Vec::new(),
        }
    }
    pub fn add_free_entry(&mut self, obj_nr: usize, next_free: usize) {
        self.entries.push(XrefEntry::Free{obj_nr: obj_nr, next_free: next_free});
    }
    pub fn add_inuse_entry(&mut self, pos: usize, gen_nr: usize) {
        self.entries.push(XrefEntry::InUse{pos: pos, gen_nr: gen_nr});
    }
}

/* Objects */
pub struct IndirectObject {
    pub obj_nr: i32,
    pub gen_nr: i32,
    pub object: Object,
}
pub enum Object {
    Integer(i32),
    RealNumber(f32),
    Boolean(bool),
    String(StringType, String),
    Stream {filters: Vec<Name>, dictionary: Vec<(Name, Object)>, content: String},
    Dictionary(Vec<(Name, Object)>),
    Array(Vec<Object>),
    Reference {obj_nr: i32, gen_nr: i32},
    Null,
}

impl Object {
    /// `self` must be an `Object::Dictionary`.
    pub fn dictionary_get<'a>(&'a self, key: Name) -> Option<&'a Object> {
        match self {
            &Object::Dictionary(ref dictionary) => {
                for &(ref name, ref object) in dictionary {
                    if key.0 == name.0 {
                        return Some(object);
                    }
                }
                None
            },
            _ => {
                panic!("dictionary_get called on an Object that is not Object::Dictionary.");
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
            &Object::Stream{filters: _, dictionary: _, content: ref content} => "Stream(".to_string() + content.as_str() + ")",
            &Object::Dictionary(_) => "Object::Dictionary".to_string(),
            _ => "Some other object".to_string(),
        }
    }
}

pub struct Name(pub String); // Is technically an object but I keep it outside for now
// TODO Name could be an enum if Names are really a known finite set. Easy comparision

pub enum StringType {
    HEX, UTF8
}
pub enum Filter {
    ASCIIHexDecode,
    ASCII85Decode,
    // etc...
}
