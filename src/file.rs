use std::str;
use std::io::Read;
use std::ops::{Range};

use err::*;
use object::*;
use types::*;
use xref::{XRef, XRefTable};
use primitive::{Primitive, Stream, Dictionary, PdfString};
use backend::Backend;
use parser::parse;
use parser::parse_object::parse_indirect_object;
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
            let (xref_sections, trailer) = read_xref_and_trailer_at(&mut lexer, NO_RESOLVE)?;
            let highest_id = trailer.get("Size").ok_or_else(|| ErrorKind::EntryNotFound {key: "Size"})?.clone().as_integer()?;

            let mut refs = XRefTable::new(highest_id as ObjNr);
            for section in xref_sections {
                refs.add_entries_from(section);
            }
            
            println!("XRefTable: {:?}", refs);
            println!("Trailer dict: {:?}", trailer);
            (refs, trailer)
        };
        let trailer = Trailer::from_dict(trailer, &|r| File::<B>::resolve_helper(&backend, &refs, r))?;
        
        Ok(File {
            backend: backend,
            trailer: trailer,
            refs: refs,
        })
    }

    pub fn get_root(&self) -> &Catalog {
        &self.trailer.root
    }

    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        File::<B>::resolve_helper(&self.backend, &self.refs, r)
    }

    /// Because we need a resolve function to parse the trailer before the File has been created.
    fn resolve_helper<B2: Backend>(backend: &B2, refs: &XRefTable, r: PlainRef) -> Result<Primitive> {
        match refs.get(r.id)? {
            XRef::Raw {pos, gen_nr} => {
                let mut lexer = Lexer::new(backend.read(pos..)?);
                Ok(parse_indirect_object(&mut lexer)?.1)
            }
            XRef::Stream {stream_id, index} => {
                let obj_stream = File::<B2>::resolve_helper(backend, refs, PlainRef {id: stream_id, gen: 0 /* TODO what gen nr? */})?;
                let obj_stream = ObjectStream::from_primitive(obj_stream, &|r| File::<B>::resolve_helper(backend, refs, r))?;
                let slice = obj_stream.get_object_slice(index)?;
                parse(slice)
            }
            XRef::Free {..} => bail!("Object is free"),
        }
    }

    pub fn deref<T: FromPrimitive>(&self, r: Ref<T>) -> Result<T> {
        let primitive = self.resolve(r.get_inner())?;
        T::from_primitive(primitive, &|id| self.resolve(id))
    }
    pub fn get_num_pages(&self) -> Result<i32> {
        Ok(self.trailer.root.pages.count)
    }
    pub fn get_page(&self, n: i32) -> Result<&Page> {
        if n >= self.get_num_pages()? {
            return Err(ErrorKind::OutOfBounds.into());
        }
        self.find_page(n, 0, &self.trailer.root.pages)
    }
    fn find_page<'a>(&'a self, page_nr: i32, mut offset: i32, pages: &'a Pages) -> Result<&'a Page> {
        for kid in &pages.kids {
            println!("{}/{} {:?}", offset, page_nr, kid);
            match *kid {
                PagesNode::Tree(ref t) => {
                    if offset + t.count < page_nr {
                        offset += t.count;
                    } else {
                        self.find_page(page_nr, offset, t);
                    }
                },
                PagesNode::Leaf(ref p) => {
                    if offset > page_nr {
                        offset += 1;
                    } else {
                        assert_eq!(offset, page_nr);
                        return Ok(p);
                    }
                }
            }
        }
        bail!("not found!");
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

#[derive(Object, FromDict)]
#[pdf(Type=false)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id:         i32,

    #[pdf(key = "Prev", opt = true)]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               Catalog,

    #[pdf(key = "Encrypt", opt = true)]
    pub encrypt_dict:       Option<Dictionary>,

    #[pdf(key = "Info", opt = true)]
    pub info_dict:          Option<Dictionary>,

    #[pdf(key = "ID", opt = true)]
    pub id:                 Option<Vec<PdfString>>,
    // TODO ^ Vec<u8> is a String type. Maybe make a wrapper for that
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

    #[pdf(key = "Index", opt = true)]
    /// Array of pairs of integers for each subsection, (first object number, number of entries).
    /// Default value (assumed when None): `(0, self.size)`.
    pub index: Option<Vec<i32>>,

    #[pdf(key = "Prev", opt = true)]
    prev: Option<i32>,

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
    /// Number of compressed objects in the stream.
    pub num_objects: i32,

    #[pdf(key = "First")]
    /// The byte offset in the decoded stream, of the first compressed object.
    pub first: i32,

    #[pdf(key = "Extends", opt=true)]
    /// A reference to an eventual ObjectStream which this ObjectStream extends.
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
        let start = self.info.first as usize + self.offsets[index];
        let end = if index == self.offsets.len() - 1 {
            self.data.len()
        } else {
            self.info.first as usize + self.offsets[index + 1]
        };

        Ok(&self.data[start..end])
    }
}

impl FromStream for ObjectStream {
    fn from_stream(stream: Stream, resolve: &Resolve) -> Result<Self> {
        let info = ObjStmInfo::from_dict(stream.info, resolve)?;
        let data = stream.data.to_vec();

        let mut offsets = Vec::new();
        {
            let mut lexer = Lexer::new(&data);
            for i in 0..(info.num_objects as ObjNr) {
                let obj_nr = lexer.next()?.to::<ObjNr>()?;
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

impl FromPrimitive for ObjectStream {
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<ObjectStream> {
        ObjectStream::from_stream(p.as_stream(r)?, r)
    }
}


#[cfg(test)]
mod tests {
    use std::str;
    use file::{File, ObjectStream};
    use object::{FromPrimitive, NO_RESOLVE};
    use memmap::Mmap;
    use pdf::print_err;
    use object::PlainRef;
    use parser::parse;


    #[test]
    fn new_File() {
        let _ = File::<Vec<u8>>::open("example.pdf").unwrap();
        let _ = File::<Mmap>::open("example.pdf").unwrap();
    }

    #[test]
    fn read_pages() {
        let file = File::<Vec<u8>>::open("example.pdf").unwrap();
        let num_pages = file.trailer.root.pages.count;
        for i in 0..num_pages {
            println!("Read page {}", i);
            let page = file.get_page(i);
        }
    }

    #[test]
    fn parse_objects_from_stream() {
        let file = File::<Vec<u8>>::open("la.pdf").unwrap();
        let obj_stream = file.resolve(PlainRef {id: 13, gen: 0}).unwrap();
        let obj_stream = ObjectStream::from_primitive(obj_stream, NO_RESOLVE).unwrap();
        for i in 0..obj_stream.offsets.len() {
            let slice = obj_stream.get_object_slice(i).unwrap();
            println!("Object slice #{}: {}", i, str::from_utf8(slice).unwrap());
            println!();
            parse(slice).unwrap();
        }
    }

    #[test]
    fn flate_decode() {
        let file = File::<Vec<u8>>::open("la.pdf").unwrap();
        let num_pages = file.trailer.root.pages.count;
        for i in 0..num_pages {
            println!("Read page {}", i);
            let page = file.get_page(i);
        }
    }
}
