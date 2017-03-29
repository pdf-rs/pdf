use std::str;
use std::io::Read;
use std::ops::{Range};

use err::*;
use object::*;
use types::*;
use xref::{XRef, XRefTable};
use primitive::{Primitive, Stream, Dictionary};
use backend::Backend;
use parser::parse;
use parser::lexer::Lexer;
use parser::parse_xref::read_xref_and_trailer_at;


pub struct File<B: Backend> {
    backend:    B,
    trailer:    Trailer,
    refs:       XRefTable,
}


impl<B: Backend> File<B> {
    pub fn open(path: &str) -> Result<File<B>> {
        let backend = B::open(path)?;
        let xref_offset = locate_xref_offset(backend.read(0..)?)?;


        // TODO: lexer may have to go before xref_offset? Investigate this.
        //      Reason for the doubt: reading previous xref tables/streams
        let (refs, trailer) = {
            let mut lexer = Lexer::new(backend.read(xref_offset..)?);
            let (xref_sections, trailer) = read_xref_and_trailer_at(&mut lexer)?;

            let mut refs = XRefTable::new(trailer.highest_id as ObjNr);
            for section in xref_sections {
                refs.add_entries_from(section);
            }
            
            (refs, trailer)
        };
        
        Ok(File {
            backend: backend,
            trailer: trailer,
            refs: refs,
        })
    }

    pub fn get_root(&self) -> Result<Root> {
        self.read_object(self.trailer.root)
    }

    fn read_primitive(&self, r: PlainRef) -> Result<Primitive> {
        match self.refs.get(r.id)? {
            XRef::Raw {pos, gen_nr} => parse(self.backend.read(pos..)?),
            XRef::Stream {stream_id, index} => {
                unimplemented!();
                // let obj_stream = self.read_object( Ref::<ObjectStream>::from_id(stream_id), NO_RESOLVE)?;
                // parse(obj_stream.get_object_slice(index)?)
            }
            XRef::Free {..} => bail!("Object is free"),
        }
    }

    pub fn read_object<T: FromPrimitive>(&self, r: Ref<T>) -> Result<T> {
        let primitive = self.read_primitive(r.get_inner())?;
        T::from_primitive(primitive, &|id| self.resolve_primitive(id))
    }

    fn resolve_primitive(&self, reference: PlainRef) -> Result<Primitive> {
        unimplemented!();
    }
    pub fn resolve<T: FromPrimitive>(&self, reference: Ref<T>) -> Result<T> {
        let primitive = self.resolve_primitive(reference.get_inner())?;
        let resolve = |id| self.resolve_primitive(id);
        T::from_primitive(primitive, &resolve)
    }
    // TODO: resolve(Ref<T>) -> T???

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

#[derive(Object, FromDict)]
#[pdf(Type=false)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id:         i32,

    #[pdf(key = "Prev", opt = true)]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               Ref<Root>,

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
    fn from_stream(stream: Stream, resolve: &Resolve) -> Result<XRefStream> {
        let info = XRefInfo::from_dict(stream.info, resolve)?;
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

impl ObjectStream {
    pub fn get_object_slice(&self, index: usize) -> Result<&[u8]> {
        if index >= self.offsets.len() {
            bail!("Index into ObjectStream out of bounds.");
        }
        let start = self.offsets[index];
        let end = if index == self.offsets.len() - 1 {
            self.data.len()
        } else {
            self.offsets[index + 1]
        };

        Ok(&self.data[start..end])
    }
}

impl FromStream for ObjectStream {
    fn from_stream(stream: Stream, resolve: &Resolve) -> Result<Self> {
        let info = ObjStmInfo::from_dict(stream.info, resolve)?;
        // TODO: Look at filters of `info` and decode the stream.
        let data = stream.data.to_vec();

        let mut offsets = Vec::new();
        {
            let mut lexer = Lexer::new(&data);
            for i in 0..(info.n as ObjNr) {
                let obj_nr = lexer.next()?.to::<ObjNr>()?;
                if i != obj_nr {
                    bail!("(TODO, incomplete): Assumption violated: that the Object Stream only has consequtive objects numbers starting from 0.");
                }
                let offset = lexer.next()?.to::<usize>()?;
                offsets.push(offset);
            }
        }
        Ok(ObjectStream {
            data: data,
            info: info,
            offsets: offsets,
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
        let file = File::<Vec<u8>>::open("example.pdf").unwrap();
        let root = file.get_root();
    }
}
