use datasize::DataSize;

use crate as pdf;
use crate::object::*;
use crate::primitive::*;
use crate::error::*;
use crate::parser::Lexer;
use crate::enc::{StreamFilter, decode};

use std::ops::{Deref, Range};
use std::fmt;

#[derive(Clone)]
pub (crate) enum StreamData {
    Generated(Arc<[u8]>),
    Original(Range<usize>, PlainRef),
}
datasize::non_dynamic_const_heap_size!(StreamData, std::mem::size_of::<StreamData>());

/// Simple Stream object with only some additional entries from the stream dict (I).
#[derive(Clone, DataSize)]
pub struct Stream<I> {
    pub info: StreamInfo<I>,
    pub (crate) inner_data: StreamData,
}
impl<I: Object> Stream<I> {
    pub fn from_stream(s: PdfStream, resolve: &impl Resolve) -> Result<Self> {
        let PdfStream {info, inner} = s;
        let info = StreamInfo::<I>::from_primitive(Primitive::Dictionary (info), resolve)?;
        let inner_data = match inner {
            StreamInner::InFile { id, file_range } => StreamData::Original(file_range, id),
            StreamInner::Pending { data } => StreamData::Generated(data)
        };
        Ok(Stream { info, inner_data })
    }

    /// the data is not compressed. the specified filters are to be applied when compressing the data
    pub fn new_with_filters(i: I, data: impl Into<Arc<[u8]>>, filters: Vec<StreamFilter>) -> Stream<I> {
        Stream {
            info: StreamInfo {
                filters,
                file: None,
                file_filters: Vec::new(),
                info: i
            },
            inner_data: StreamData::Generated(data.into()),
        }
    }
    pub fn new(i: I, data: impl Into<Arc<[u8]>>) -> Stream<I> {
        Stream {
            info: StreamInfo {
                filters: Vec::new(),
                file: None,
                file_filters: Vec::new(),
                info: i
            },
            inner_data: StreamData::Generated(data.into()),
        }
    }
    /// the data is already compressed with the specified filters
    pub fn from_compressed(i: I, data: impl Into<Arc<[u8]>>, filters: Vec<StreamFilter>) -> Stream<I> {
        Stream {
            info: StreamInfo {
                filters: filters.clone(),
                file: None,
                file_filters: Vec::new(),
                info: i
            },
            inner_data: StreamData::Generated(data.into()),
        }
    }

    pub fn data(&self, resolve: &impl Resolve) -> Result<Arc<[u8]>> {
        match self.inner_data {
            StreamData::Generated(ref data) => {
                let filters = &self.info.filters;
                if filters.len() == 0 {
                    Ok(data.clone())
                } else {
                    use std::borrow::Cow;
                    let mut data: Cow<[u8]> = (&**data).into();
                    for filter in filters {
                        data = t!(decode(&data, filter), filter).into();
                    }
                    Ok(data.into())
                }
            }
            StreamData::Original(ref file_range, id) => {
                resolve.get_data_or_decode(id, file_range.clone(), &self.info.filters)
            }
        }
    }

    pub fn len(&self) -> usize {
        match self.inner_data {
            StreamData::Generated(ref data) => data.len(),
            StreamData::Original(ref range, _) => range.len()
        }
    }
}

impl<I: Object + fmt::Debug> fmt::Debug for Stream<I> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Stream info={:?}, len={}", self.info.info, self.len())
    }
}

impl<I: Object> Object for Stream<I> {
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
                    StreamFilter::CCITTFaxDecode(ref p) => Some(p.to_primitive(update)?),
                    StreamFilter::JBIG2Decode(ref p) => Some(p.to_primitive(update)?),
                    _ => None
                } {
                    assert!(params.is_none());
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
                StreamFilter::JBIG2Decode(ref _p) => "JBIG2Decode",
                StreamFilter::Crypt => "Crypt",
                StreamFilter::RunLengthDecode => "RunLengthDecode",
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

        let inner = match self.inner_data {
            StreamData::Generated(ref data) => {
                info.insert("Length", Primitive::Integer(data.len() as _));
                StreamInner::Pending { data: data.clone() }
            },
            StreamData::Original(ref file_range, id) => {
                info.insert("Length", Primitive::Integer(file_range.len() as _));
                StreamInner::InFile { id, file_range: file_range.clone() }
            }
        };

        Ok(PdfStream { info, inner })
    }
}
impl<I: ObjectWrite> ObjectWrite for Stream<I> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self.inner_data {
            StreamData::Original(_, id) => Ok(Primitive::Reference(id)),
            _ => self.to_pdf_stream(update).map(Primitive::Stream),
        }
    }
}
impl<I: DeepClone> DeepClone for Stream<I> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        let data = match self.inner_data {
            StreamData::Generated(ref data) => data.clone(),
            StreamData::Original(ref range, id) => cloner.stream_data(id, range.clone())?
        };
        Ok(Stream {
            info: self.info.deep_clone(cloner)?,
            inner_data: StreamData::Generated(data),
        })
    }
}
impl<I: Object> Deref for Stream<I> {
    type Target = StreamInfo<I>;
    fn deref(&self) -> &StreamInfo<I> {
        &self.info
    }
}


/// General stream type. `I` is the additional information to be read from the stream dict.
#[derive(Debug, Clone, DataSize, DeepClone)]
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

        let filters = Vec::<Name>::from_primitive(
            dict.remove("Filter").unwrap_or(Primitive::Null),
            resolve)?;

        let decode_params = Vec::<Option<Dictionary>>::from_primitive(
            dict.remove("DecodeParms").unwrap_or(Primitive::Null),
            resolve)?;

        let file = Option::<FileSpec>::from_primitive(
            dict.remove("F").unwrap_or(Primitive::Null),
            resolve)?;

        let file_filters = Vec::<Name>::from_primitive(
            dict.remove("FFilter").unwrap_or(Primitive::Null),
            resolve)?;

        let file_decode_params = Vec::<Dictionary>::from_primitive(
            dict.remove("FDecodeParms").unwrap_or(Primitive::Null),
            resolve)?;


        let mut new_filters = Vec::new();
        let mut new_file_filters = Vec::new();

        for (i, filter) in filters.iter().enumerate() {
            let params = match decode_params.get(i) {
                Some(Some(params)) => params.clone(),
                _ => Dictionary::default(),
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

#[derive(Object, Default, Debug, DataSize)]
#[pdf(Type = "ObjStm")]
pub struct ObjStmInfo {
    #[pdf(key = "N")]
    /// Number of compressed objects in the stream.
    pub num_objects: usize,

    #[pdf(key = "First")]
    /// The byte offset in the decoded stream, of the first compressed object.
    pub first: usize,

    #[pdf(key = "Extends")]
    /// A reference to an eventual ObjectStream which this ObjectStream extends.
    pub extends: Option<Ref<Stream<()>>>,
}

#[derive(DataSize)]
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
            let data = stream.data(resolve)?;
            let mut lexer = Lexer::new(&data);
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
    pub fn get_object_slice(&self, index: usize, resolve: &impl Resolve) -> Result<(Arc<[u8]>, Range<usize>)> {
        if index >= self.offsets.len() {
            err!(PdfError::ObjStmOutOfBounds {index, max: self.offsets.len()});
        }
        let start = self.inner.info.first + self.offsets[index];
        let data = self.inner.data(resolve)?;
        let end = if index == self.offsets.len() - 1 {
            data.len()
        } else {
            self.inner.info.first + self.offsets[index + 1]
        };

        Ok((data, start..end))
    }
    /// Returns the number of contained objects
    pub fn n_objects(&self) -> usize {
        self.offsets.len()
    }
    pub fn _data(&self, resolve: &impl Resolve) -> Result<Arc<[u8]>> {
        self.inner.data(resolve)
    }
}
