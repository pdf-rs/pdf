use std::io;

use object::{Object, FromStream, FromDict, Resolve};
use primitive::{Stream};
use types::StreamFilter;
use err::Result;

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
