use crate as pdf;
use crate::object::*;
use crate::primitive::*;
use crate::error::*;
use crate::parser::Lexer;
use crate::enc::{self, decode};

use once_cell::unsync::OnceCell;

use std::borrow::Cow;
use std::ops::Deref;
use std::fmt;



/// Simple Stream object with only some additional entries from the stream dict (I).
#[derive(Clone)]
pub struct Stream<I=()> {
    pub info: StreamInfo<I>,
    raw_data: Vec<u8>,
    decoded: OnceCell<Vec<u8>>
}
impl<I: Object + fmt::Debug> Stream<I> {
    pub fn from_stream(s: PdfStream, resolve: &impl Resolve) -> Result<Self> {
        let PdfStream {info, data} = s;
        let info = StreamInfo::<I>::from_primitive(Primitive::Dictionary (info), resolve)?;
        Ok(Stream { info, raw_data: data, decoded: OnceCell::new() })
    }

    pub fn new_with_filters(i: I, data: Vec<u8>, filters: Vec<StreamFilter>) -> Stream<I> {
        Stream {
            info: StreamInfo {
                filters,
                file: None,
                file_filters: Vec::new(),
                info: i
            },
            raw_data: data,
            decoded: OnceCell::new()
        }
    }
    pub fn new(i: I, data: Vec<u8>) -> Stream<I> {
        Stream {
            info: StreamInfo {
                filters: Vec::new(),
                file: None,
                file_filters: Vec::new(),
                info: i
            },
            raw_data: data,
            decoded: OnceCell::new()
        }
    }

    /// decode the data.
    /// does not store the result.
    /// The caller is responsible for caching the result
    pub fn decode(&self) -> Result<Cow<[u8]>> {
        let mut data = Cow::Borrowed(&*self.raw_data);
        for filter in &self.info.filters {
            data = match decode(&*data, filter) {
                Ok(data) => data.into(),
                Err(e) => {
                    info!("Stream Info: {:?}", &self.info);
                    dump_data(&data);
                    return Err(e);
                }
            };
        }
        Ok(data)
    }
    pub fn data(&self) -> Result<&[u8]> {
        self.decoded.get_or_try_init(|| {
            let data = self.decode()?;
            Ok(data.into_owned())
        }).map(|v| v.as_slice())
    }

    /// If this is contains DCT encoded data, return the compressed data as is
    pub fn as_jpeg(&self) -> Option<&[u8]> {
        match *self.info.filters.as_slice() {
            [StreamFilter::DCTDecode(_)] => Some(self.raw_data.as_slice()),
            _ => None
        }
    }

    pub fn hexencode(mut self) -> Self {
        self.raw_data = enc::encode_hex(&self.raw_data);
        self.info.filters.push(StreamFilter::ASCIIHexDecode);
        self
    }
}

impl<I: Object + fmt::Debug> fmt::Debug for Stream<I> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.info.info.fmt(f)
    }
}

impl<I: Object + fmt::Debug> Object for Stream<I> {
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let s = PdfStream::from_primitive(p, resolve)?;
        Stream::from_stream(s, resolve)
    }
}
impl<I: ObjectWrite> Stream<I> {
    pub fn to_pdf_stream(&self, update: &mut impl Updater) -> Result<PdfStream> {
        let mut info = match self.info.info.to_primitive(update)? {
            Primitive::Dictionary(dict) => dict,
            Primitive::Null => Dictionary::new(),
            p => bail!("stream info has to be a dictionary (found {:?})", p)
        };
        let mut params = None;
        if self.info.filters.len() > 0 {
            for f in self.info.filters.iter() {
                if let Some(para) = match f {
                    StreamFilter::LZWDecode(ref p) => Some(p.to_primitive(update)?),
                    StreamFilter::FlateDecode(ref p) => Some(p.to_primitive(update)?),
                    StreamFilter::DCTDecode(ref p) => Some(p.to_primitive(update)?),
                    _ => None
                } {
                    if params.is_some() {
                        panic!();
                    }
                    params = Some(para);
                }
            }
            let mut filters = self.info.filters.iter().map(|filter| match filter {
                StreamFilter::ASCIIHexDecode => "ASCIIHexDecode",
                StreamFilter::ASCII85Decode => "ASCII85Decode",
                StreamFilter::LZWDecode(ref _p) => "LZWDecode",
                StreamFilter::FlateDecode(ref _p) => "FlateDecode",
                StreamFilter::JPXDecode => "JPXDecode",
                StreamFilter::DCTDecode(ref _p) => "DCTDecode",
                StreamFilter::CCITTFaxDecode(ref _p) => "CCITTFaxDecode",
                StreamFilter::Crypt => "Crypt",
            })
            .map(|s| Primitive::Name(s.into()));
            match self.info.filters.len() {
                0 => {},
                1 => {
                    info.insert("Filter", filters.next().unwrap().to_primitive(update)?);
                }
                _ => {
                    info.insert("Filter", Primitive::array::<Primitive, _, _, _>(filters, update)?);
                }
            }
        }
        if let Some(para) = params {
            info.insert("DecodeParms", para);
        }
        info.insert("Length", Primitive::Integer(self.raw_data.len() as _));

        Ok(PdfStream {
            info,
            data: self.raw_data.clone()
        })
    }
}
impl<I: ObjectWrite> ObjectWrite for Stream<I> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.to_pdf_stream(update).map(Primitive::Stream)
    }
}

impl<I: Object> Deref for Stream<I> {
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
    pub info: I,
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
/*
    /// If the stream is not encoded, this is a no-op. `decode()` should be called whenever it's uncertain
    /// whether the stream is encoded.
    pub fn encode(&mut self, _filter: StreamFilter) {
        // TODO this should add the filter to `self.filters` and encode the data with the given
        // filter
        unimplemented!();
    }*/
    pub fn get_filters(&self) -> &[StreamFilter] {
        &self.filters
    }
}
impl<T: Object> Object for StreamInfo<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = Dictionary::from_primitive(p, resolve)?;

        let _length = usize::from_primitive(
            dict.remove("Length").ok_or(PdfError::MissingEntry{ typ: "StreamInfo", field: "Length".into() })?,
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
            file,
            file_filters: new_file_filters,
            // Special
            info: T::from_primitive(Primitive::Dictionary (dict), resolve)?,
        })
    }
}

#[derive(Object, Default, Debug)]
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
    pub extends: Option<Ref<Stream>>,

}


pub struct ObjectStream {
    /// Byte offset of each object. Index is the object number.
    offsets:    Vec<usize>,
    /// The object number of this object.
    _id:         ObjNr,
    
    inner:      Stream<ObjStmInfo>
}

impl Object for ObjectStream {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<ObjectStream> {
        let stream: Stream<ObjStmInfo> = Stream::from_primitive(p, resolve)?;

        let mut offsets = Vec::new();
        {
            debug!("parsing stream");
            let mut lexer = Lexer::new(stream.data()?);
            for _ in 0..(stream.info.num_objects as ObjNr) {
                let _obj_nr = lexer.next()?.to::<ObjNr>()?;
                let offset = lexer.next()?.to::<usize>()?;
                offsets.push(offset);
            }
        }

        Ok(ObjectStream {
            offsets,
            _id: 0, // TODO
            inner: stream
        })
    }
}

impl ObjectStream {
    pub fn get_object_slice(&self, index: usize) -> Result<&[u8]> {
        if index >= self.offsets.len() {
            err!(PdfError::ObjStmOutOfBounds {index, max: self.offsets.len()});
        }
        let start = self.inner.info.first as usize + self.offsets[index];
        let data = self.inner.data()?;
        let end = if index == self.offsets.len() - 1 {
            data.len()
        } else {
            self.inner.info.first as usize + self.offsets[index + 1]
        };

        Ok(&data[start..end])
    }
    /// Returns the number of contained objects
    pub fn n_objects(&self) -> usize {
        self.offsets.len()
    }
}
