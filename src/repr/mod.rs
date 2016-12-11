//! Runtime representation of a PDF file.

use std::io;
use std::vec::Vec;
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
#[derive(Default)]
pub struct XrefTable {
    pub first_entry: usize,
    pub entries: Vec<XrefEntry>,
}
pub enum XrefEntry {
    Free{obj_num: usize, next_free: usize},
    InUse{pos: usize, gen_num: usize},
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
    Stream {filters: Vec<Name>, dictionary: Vec<(Name, Object)>, contents: String},
    Dictionary(Vec<(Name, Object)>),
    Array(Vec<Object>),
    Reference {obj_nr: i32, gen_nr: i32},
    Null,
}
pub struct Name(String); // Is technically an object but I keep it outside for now
// TODO Name could be an enum if Names are really a known finite set. Easy comparision

pub enum StringType {
    HEX, UTF8
}
pub enum Filter {
    ASCIIHexDecode,
    ASCII85Decode,
    // etc...
}
