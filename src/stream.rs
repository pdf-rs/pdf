use std::collections::HashMap;
use std::io;

use file::File;
use object::{Object, PrimitiveConv, FromStream, Resolve};
use primitive::{Primitive, Stream};
use types::StreamFilter;
use inflate::InflateStream;

#[derive(Object, PrimitiveConv)]
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
        let info = StreamInfo::from_dict(stream.info, resolve)?;
        // TODO: Look at filters of `info` and decode the stream.
        let data = stream.data.to_vec();
        Ok(GeneralStream {
            data: data,
            info: info,
        })
    }
}


// TODO move out to decoding/encoding module
fn flat_decode(data: &[u8]) -> Vec<u8> {
    let mut inflater = InflateStream::from_zlib();
    let mut out = Vec::<u8>::new();
    let mut n = 0;
    while n < data.len() {
        let res = inflater.update(&data[n..]);
        if let Ok((num_bytes_read, result)) = res {
            n += num_bytes_read;
            out.extend(result);
        } else {
            res.unwrap();
        }
    }
    out
}
