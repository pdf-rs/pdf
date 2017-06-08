use std::io;

use object::{Object, FromStream, FromDict, Resolve};
use primitive::Stream;
use types::StreamFilter;
use err::Result;

#[derive(Object, FromDict)]
pub struct StreamInfo {
    #[pdf(key = "Filter")]
    pub filter: Vec<StreamFilter>,

    // TODO The Array can also have Nulls.. would this be achieved with Option<DecodeParms>?
    #[pdf(key = "DecodeParms", opt=true)]
    pub decode_parms: Option<Vec<DecodeParams>>,
    
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
                decode_parms:   None,
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
    fn from_stream(stream: Stream, resolve: &Resolve)
     -> Result<GeneralStream>
    {
        let info = StreamInfo::from_dict(stream.info, resolve)?;
        // TODO: Look at filters of `info` and decode the stream.
        let data = stream.data.to_vec();
        Ok(GeneralStream {
            data: data,
            info: info,
        })
    }
}


// TODO the following should probably be an enum, because parameters are different for most filter
// types. The following is only for Flate/LZW
#[derive(Object, FromDict)]
pub struct DecodeParams {
    #[pdf(key = "Predictor")]
    predictor: i32,
    #[pdf(key = "Colors", opt = true)]
    /// Only if Predictor > 1
    n_components: Option<i32>,
    #[pdf(key = "BitsPerComponent", opt = true)]
    /// Only if Predictor > 1
    bits_per_component: Option<i32>,
    #[pdf(key = "Columns", opt = true)]
    /// Only if Predictor > 1
    columns: Option<i32>,
    #[pdf(key = "EarlyChange", opt = true)]
    /// LZWDecode only
    early_change: Option<i32>,
}

impl Default for DecodeParams {
    // TODO should be possible to have fields have default values rather than opt=true
    fn default() -> DecodeParams {
        DecodeParams {
            predictor: 1,
            n_components: Some(1),
            bits_per_component: Some(8),
            columns: Some(1),
            early_change: Some(1),
        }
    }
}
