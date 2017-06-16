use std::io::{self, Write};
use std::collections::HashMap;
use object::{Object, FromStream, FromDict, Resolve, FromPrimitive, ObjNr, PlainRef};
use primitive::{Stream, Primitive, Dictionary};
use types::StreamFilter;
use err::*;
use parser::lexer::Lexer;
use backend::Backend;
use file::File;

#[derive(Object, FromDict)]
pub struct StreamInfo {
    #[pdf(key = "Filter")]
    pub filter: Vec<StreamFilter>,

    // #[pdf(key = "DecodeParms", opt=true)]
    // pub decode_parms: Option<Vec<Option<DecodeParams>>>,
    
    #[pdf(key = "Type")]
    ty:     String
}


pub struct GeneralStream {
    pub data:       Vec<u8>,
    pub info:       StreamInfo
}
impl GeneralStream {
    pub fn empty(ty: &str) -> GeneralStream {
        GeneralStream {
            data:   Vec::new(),
            info:   StreamInfo {
                filter:         vec![],
                ty:             ty.to_string()
            }
        }
    }
}
impl Object for GeneralStream {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        self.info.serialize(out)?;
        
        out.write(b"stream\n")?;
        out.write(&self.data)?;
        out.write(b"\nendstream\n")?;
        Ok(())
    }
}

impl FromStream for GeneralStream {
    fn from_stream(stream: Stream, resolve: &Resolve) -> Result<GeneralStream> {
        let info = StreamInfo::from_dict(stream.info, resolve)?;
        // TODO: Look at filters of `info` and decode the stream.
        let data = stream.data.to_vec();
        Ok(GeneralStream {
            data: data,
            info: info,
        })
    }
}

/*
pub struct DecodeParams {
    dict: Dictionary
}

impl DecodeParams {
    fn get(&self, key: String) -> Option<Primitive> {
        self.dict.get(key)
    }
}
*/

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

#[allow(dead_code)]
pub struct ObjectStream {
    pub data:       Vec<u8>,
    /// Fields in the stream dictionary.
    pub info:       ObjStmInfo,
    /// Byte offset of each object. Index is the object number.
    offsets:    Vec<usize>,
    /// The object number of this object.
    id:         ObjNr,
}
impl Object for ObjectStream {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        self.info.serialize(out)?;
        
        out.write(b"stream\n")?;
        out.write(&self.data)?;
        out.write(b"\nendstream\n")?;
        Ok(())
    }
}

impl ObjectStream {
    pub fn new<B: Backend>(file: &mut File<B>) -> ObjectStream {
        let self_ref: PlainRef = (&file.promise::<ObjectStream>()).into();
        ObjectStream {
            data:       Vec::new(),
            info:       ObjStmInfo::default(),
            offsets:    Vec::new(),
            id:         self_ref.id
        }
    }
    pub fn id(&self) -> ObjNr {
        self.id
    }
    pub fn get_object_slice(&self, index: usize) -> Result<&[u8]> {
        if index >= self.offsets.len() {
            bail!(ErrorKind::ObjStmOutOfBounds {index: index, max: self.offsets.len()});
        }
        let start = self.info.first as usize + self.offsets[index];
        let end = if index == self.offsets.len() - 1 {
            self.data.len()
        } else {
            self.info.first as usize + self.offsets[index + 1]
        };

        Ok(&self.data[start..end])
    }
    /// Returns the number of contained objects
    pub fn n_objects(&self) -> usize {
        self.offsets.len()
    }
}
impl Into<Primitive> for ObjectStream {
    fn into(self) -> Primitive {
        let mut data: Vec<u8> = vec![];
        let mut offsets_iter = self.offsets.iter().cloned();
        if let Some(first) = offsets_iter.next() {
            write!(data, "{}", first);
            for o in offsets_iter {
                write!(data, " {}", o);
            }
        }
        write!(data, "\n");
        let first = data.len();
        
        data.extend_from_slice(&self.data);
        
        
        let mut info = Dictionary::new();
        info.insert("Type".into(), Primitive::Name("ObjStm".into()));
        info.insert("Length".into(), Primitive::Integer(data.len() as i32));
        info.insert("Filter".into(), Primitive::Null);
        info.insert("N".into(), Primitive::Integer(self.offsets.len() as i32));
        info.insert("First".into(), Primitive::Integer(first as i32));
        
        Primitive::Stream(Stream {
            info: info,
            data: data
        })
    }
}

impl FromStream for ObjectStream {
    fn from_stream(stream: Stream, resolve: &Resolve) -> Result<Self> {
        let info = ObjStmInfo::from_dict(stream.info, resolve)?;
        let data = stream.data.to_vec();

        let mut offsets = Vec::new();
        {
            let mut lexer = Lexer::new(&data);
            for _ in 0..(info.num_objects as ObjNr) {
                let _obj_nr = lexer.next()?.to::<ObjNr>()?;
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
