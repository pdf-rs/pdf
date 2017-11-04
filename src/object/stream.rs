use object::*;
use primitive::*;
use err::*;
use parser::Lexer;
use enc::decode;


use std::io;
use std::ops::Deref;

/// Simple Stream object with only some additional entries from the stream dict (I).
#[derive(Debug,Clone)]
pub struct Stream<I: Object> {
    pub info: StreamInfo<I>,
    pub data: Vec<u8>,
}

impl<I: Object> Object for Stream<I> {
    /// Write object as a byte stream
    fn serialize<W: io::Write>(&self, _: &mut W) -> io::Result<()> {unimplemented!()}
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        let PdfStream {info, data} = PdfStream::from_primitive(p, resolve)?;
        let info = StreamInfo::<I>::from_primitive(Primitive::Dictionary (info), resolve)?;
        Ok(Stream {
            info: info,
            data: data,
        })
    }
}

impl<I: Object> Stream<I> {
    pub fn decode(&mut self) -> Result<()> {
        for filter in &self.info.filters {
            eprintln!("Decode filter: {:?}", filter);
            self.data = decode(&self.data, filter)?;
        }
        self.info.filters.clear();
        Ok(())
    }
}

impl<I:Object> Deref for Stream<I> {
    type Target = StreamInfo<I>;
    fn deref(&self) -> &StreamInfo<I> {
        &self.info
    }
}


/// General stream type. `I` is the additional information to be read from the stream dict.
#[derive(Debug, Clone)]
pub struct StreamInfo<I> {
    // General dictionary entries
    /// Filters that the `data` is currently encoded with (corresponds to both `/Filter` and
    /// `/DecodeParms` in the PDF specs), constructed in `from_primitive()`.
    pub filters: Vec<StreamFilter>,

    /// Eventual file containing the stream contentst
    pub file: Option<FileSpec>,
    /// Filters to apply to external file specified in `file`.
    pub file_filters: Vec<StreamFilter>,

    // TODO:
    /*
    /// Filters to apply to external file specified in `file`.
    #[pdf(key="FFilter")]
    file_filters: Vec<StreamFilter>,
    #[pdf(key="FDecodeParms")]
    file_decode_parms: Vec<DecodeParms>,
    /// Number of bytes in the decoded stream
    #[pdf(key="DL")]
    dl: Option<usize>,
    */
    // Specialized dictionary entries
    info: I,
}

impl<I> Deref for StreamInfo<I> {
    type Target = I;
    fn deref(&self) -> &I {
        &self.info
    }
}

impl<I: Default> Default for StreamInfo<I> {
    fn default() -> StreamInfo<I> {
        StreamInfo {
            filters: Vec::new(),
            file: None,
            file_filters: Vec::new(),
            info: I::default(),
        }
    }
}
impl<T> StreamInfo<T> {
    /// If the stream is not encoded, this is a no-op. `decode()` should be called whenever it's uncertain
    /// whether the stream is encoded.
    pub fn encode(&mut self, _filter: StreamFilter) {
        // TODO this should add the filter to `self.filters` and encode the data with the given
        // filter
        unimplemented!();
    }
    pub fn get_filters(&self) -> &[StreamFilter] {
        &self.filters
    }
}
impl<T: Object> Object for StreamInfo<T> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        let mut dict = Dictionary::from_primitive(p, resolve)?;

        let _length = usize::from_primitive(
            dict.remove("Length").ok_or(Error::from(ErrorKind::EntryNotFound{key:"Length"}))?,
            resolve)?;

        let filters = Vec::<String>::from_primitive(
            dict.remove("Filter").or(Some(Primitive::Null)).unwrap(),
            resolve)?;

        let decode_params = Vec::<Dictionary>::from_primitive(
            dict.remove("DecodeParms").or(Some(Primitive::Null)).unwrap(),
            resolve)?;

        let file = Option::<FileSpec>::from_primitive(
            dict.remove("F").or(Some(Primitive::Null)).unwrap(),
            resolve)?;

        let file_filters = Vec::<String>::from_primitive(
            dict.remove("FFilter").or(Some(Primitive::Null)).unwrap(),
            resolve)?;

        let file_decode_params = Vec::<Dictionary>::from_primitive(
            dict.remove("FDecodeParms").or(Some(Primitive::Null)).unwrap(),
            resolve)?;


        let mut new_filters = Vec::new();
        let mut new_file_filters = Vec::new();

        for (i, filter) in filters.iter().enumerate() {
            let params = match decode_params.get(i) {
                Some(params) => params.clone(),
                None => Dictionary::default(),
            };
            new_filters.push(StreamFilter::from_kind_and_params(filter, params, resolve)?);
        }
        for (i, filter) in file_filters.iter().enumerate() {
            let params = match file_decode_params.get(i) {
                Some(params) => params.clone(),
                None => Dictionary::default(),
            };
            new_file_filters.push(StreamFilter::from_kind_and_params(filter, params, resolve)?);
        }

        Ok(StreamInfo {
            // General
            filters: new_filters,
            file: file,
            file_filters: new_file_filters,
            // Special
            info: T::from_primitive(Primitive::Dictionary (dict.clone()), resolve)?,
        })
    }
}


// TODO: Where should this go??
/// Decode data with all filters (should be moved)
pub fn decode_fully(data: &mut Vec<u8>, filters: &mut Vec<StreamFilter>) -> Result<()> {
    for filter in filters.iter() {
        *data = decode(&data, filter)?;
    }
    filters.clear();
    Ok(())
}

#[derive(Object, Default)]
#[pdf(Type = "ObjStm")]
pub struct ObjStmInfo {
    #[pdf(key = "N")]
    /// Number of compressed objects in the stream.
    pub num_objects: i32,

    #[pdf(key = "First")]
    /// The byte offset in the decoded stream, of the first compressed object.
    pub first: i32,

    #[pdf(key = "Extends")]
    /// A reference to an eventual ObjectStream which this ObjectStream extends.
    pub extends: Option<i32>,

}


pub struct ObjectStream {
    info:       StreamInfo<ObjStmInfo>,
    /// Byte offset of each object. Index is the object number.
    offsets:    Vec<usize>,
    /// The object number of this object.
    id:         ObjNr,
    
    data:       Vec<u8>,
}

impl Object for ObjectStream {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<ObjectStream> {
        let PdfStream {info, mut data} = PdfStream::from_primitive(p, resolve)?;
        let mut info = StreamInfo::<ObjStmInfo>::from_primitive(Primitive::Dictionary (info), resolve)?;
        decode_fully(&mut data, &mut info.filters)?;

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
            info: info,
            offsets: offsets,
            id: 0, // TODO
            data: data,
        })
    }
}

impl ObjectStream {
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
