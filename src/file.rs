use xref::XRefTable;
use std::str;
use std::io::Read;
use types::StreamFilter;
use object::*;
use primitive::{Primitive, Stream, Dictionary};
use err::*;
use parser::lexer::Lexer;
use std::ops::{Range};
use backend::Backend;
use object::Object;


pub struct File<B: Backend> {
    backend:    B,
    refs:       XRefTable,
}


impl<B: Backend> File<B> {
    fn open(path: &str) -> Result<File<B>> {
        let backend = B::open(path)?;
        let xref_offset = locate_xref_offset(backend.read(0..)?)?;
        
        Ok(File {
            backend: backend,
            refs: XRefTable::new(0),
        })
    }
}

// Returns the value of startxref
fn locate_xref_offset(data: &[u8]) -> Result<usize> {
    // locate the xref offset at the end of the file
    // `\nPOS\n%%EOF` where POS is the position encoded as base 10 integer.
    // u64::MAX has 20 digits + \n\n(2) + %%EOF(5) = 27 bytes max.

    let mut lexer = Lexer::new(data);
    lexer.set_pos_from_end(0);
    lexer.seek_substr_back(b"startxref")?;
    Ok(lexer.next()?.to::<usize>()?)
}

#[test]
fn locate_offset() {
    use std::fs::File;
    let mut buf = Vec::new();
    let mut f = File::open("example.pdf").unwrap();
    f.read_to_end(&mut buf).unwrap();
    locate_xref_offset(&buf);
}

#[derive(Object, FromDict)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id:         i32,

    #[pdf(key = "Prev", opt = true)]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               Ref<Dictionary>,

    #[pdf(key = "Encrypt", opt = true)]
    pub encrypt_dict:       Option<MaybeRef<Dictionary>>,

    #[pdf(key = "Info", opt = true)]
    pub info_dict:          Option<Ref<Dictionary>>,

    #[pdf(key = "ID", opt = true)]
    pub id:                 Option<Vec<String>>
}

impl Trailer {
}



#[derive(Object, FromDict)]
#[pdf(Type = "XRef")]
pub struct XRefInfo {
    // Normal Stream fields
    #[pdf(key = "Filter")]
    filter: Vec<StreamFilter>,

    // XRefStream fields
    #[pdf(key = "Size")]
    pub size: i32,

    #[pdf(key = "Index")]
    pub index: Vec<i32>,

    #[pdf(key = "Prev")]
    prev: i32,

    #[pdf(key = "W")]
    pub w: Vec<i32>
}

pub struct XRefStream {
    pub data: Vec<u8>,
    pub info: XRefInfo,
}

impl FromStream for XRefStream {
    fn from_stream(stream: &Stream, resolve: &Resolve) -> Result<XRefStream> {
        let info = XRefInfo::from_dict(&stream.info, resolve)?;
        // TODO: Look at filters of `info` and decode the stream.
        let data = stream.data.to_vec();
        Ok(XRefStream {
            data: data,
            info: info,
        })
    }
}


#[derive(Object, FromDict, Default)]
#[pdf(Type = "ObjStm")]
pub struct ObjStmInfo {
    // Normal Stream fields - added as fields are added to Stream
    #[pdf(key = "Filter")]
    pub filter: Vec<StreamFilter>,

    // ObjStm fields
    #[pdf(key = "N")]
    pub n: i32,

    #[pdf(key = "First")]
    pub first: i32,

    #[pdf(key = "Extends", opt=true)]
    pub extends: Option<i32>,

}

pub struct ObjectStream {
    pub data:       Vec<u8>,
    /// Fields in the stream dictionary.
    pub info:       ObjStmInfo,
    /// Byte offset of each object. Index is the object number.
    offsets:    Vec<usize>,
    /// The object number of this object.
    id:         ObjNr,
}

impl FromStream for ObjectStream {
    fn from_stream(stream: &Stream, resolve: &Resolve) -> Result<Self> {
        let info = ObjStmInfo::from_dict(&stream.info, resolve)?;
        // TODO: Look at filters of `info` and decode the stream.
        let data = stream.data.to_vec();
        Ok(ObjectStream {
            data: data,
            info: info,
            offsets: Vec::new(), // TODO: Parse from stream
            id: 0, // TODO
        })
    }
}


#[cfg(test)]
mod tests {
    use file::File;
    use memmap::Mmap;
    #[test]
    fn new_File() {
        let _ = File::<Vec<u8>>::open("example.pdf").unwrap();
        let _ = File::<Mmap>::open("example.pdf").unwrap();
    }

    #[test]
    fn read_pages() {
        let _ = File::<Vec<u8>>::open("example.pdf").unwrap();
    }
}
