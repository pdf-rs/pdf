use std::io;

use object::{Object, FromStream, FromDict, Resolve};
use primitive::{Primitive, Stream};
use types::StreamFilter;
use inflate::InflateStream;
use err::Result;

#[derive(Object, FromDict)]
pub struct StreamInfo {
    #[pdf(key = "Filter")]
    pub filter: Vec<StreamFilter>,
    
    #[pdf(key = "Type")]
    ty:     String
}


pub struct GeneralStream {
    pub data:       Vec<u8>,
    pub info:       StreamInfo
}
impl GeneralStream {
    /*
    pub fn from_file(p: &Primitive, data: &[u8]) -> Self {
        Stream {
            info:   StreamInfo::from_primitive(p),
            data:   data.to_owned()
        }
    }
    */
    pub fn empty(ty: &str) -> GeneralStream {
        GeneralStream {
            data:   Vec::new(),
            info:   StreamInfo {
                filter: vec![],
                ty:     ty.to_string()
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
    fn from_stream(stream: &Stream, resolve: &Resolve) -> Result<GeneralStream> {
        let info = StreamInfo::from_dict(&stream.info, resolve)?;
        // TODO: Look at filters of `info` and decode the stream.
        let data = stream.data.to_vec();
        Ok(GeneralStream {
            data: data,
            info: info,
        })
    }
}


