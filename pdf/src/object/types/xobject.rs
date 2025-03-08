use super::prelude::*;

#[derive(Object, Debug, DataSize, DeepClone)]
#[pdf(is_stream)]
pub enum XObject {
    #[pdf(name = "PS")]
    Postscript(PostScriptXObject),
    Image(ImageXObject),
    Form(FormXObject),
}
impl ObjectWrite for XObject {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let (subtype, mut stream) = match self {
            XObject::Postscript(s) => ("PS", s.to_pdf_stream(update)?),
            XObject::Form(s) => ("Form", s.stream.to_pdf_stream(update)?),
            XObject::Image(s) => ("Image", s.inner.to_pdf_stream(update)?),
        };
        stream.info.insert("Subtype", Name::from(subtype));
        stream.info.insert("Type", Name::from("XObject"));
        Ok(stream.into())
    }
}

/// A variant of XObject
pub type PostScriptXObject = Stream<PostScriptDict>;

#[derive(Debug, DataSize, Clone, DeepClone)]
pub struct ImageXObject {
    pub inner: Stream<ImageDict>,
}
impl Object for ImageXObject {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let s = PdfStream::from_primitive(p, resolve)?;
        Self::from_stream(s, resolve)
    }
}
impl ObjectWrite for ImageXObject {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.inner.to_primitive(update)
    }
}
impl Deref for ImageXObject {
    type Target = ImageDict;
    fn deref(&self) -> &ImageDict {
        &self.inner.info
    }
}

pub enum ImageFormat {
    Raw,
    Jpeg,
    Jp2k,
    Jbig2,
    CittFax,
    Png,
}

impl ImageXObject {
    pub fn from_stream(s: PdfStream, resolve: &impl Resolve) -> Result<Self> {
        let inner = Stream::from_stream(s, resolve)?;
        Ok(ImageXObject { inner })
    }

    /// Decode everything except for the final image encoding (jpeg, jbig2, jp2k, ...)
    pub fn raw_image_data(
        &self,
        resolve: &impl Resolve,
    ) -> Result<(Arc<[u8]>, Option<&StreamFilter>)> {
        match self.inner.inner_data {
            StreamData::Generated(_) => Ok((self.inner.data(resolve)?, None)),
            StreamData::Original(ref file_range, id) => {
                let filters = self.inner.filters.as_slice();
                // decode all non image filters
                let end = filters
                    .iter()
                    .rposition(|f| match f {
                        StreamFilter::ASCIIHexDecode => false,
                        StreamFilter::ASCII85Decode => false,
                        StreamFilter::LZWDecode(_) => false,
                        StreamFilter::RunLengthDecode => false,
                        StreamFilter::Crypt => true,
                        _ => true,
                    })
                    .unwrap_or(filters.len());

                let (normal_filters, image_filters) = filters.split_at(end);
                let data = resolve.get_data_or_decode(id, file_range.clone(), normal_filters)?;

                match image_filters {
                    [] => Ok((data, None)),
                    [StreamFilter::DCTDecode(_)]
                    | [StreamFilter::CCITTFaxDecode(_)]
                    | [StreamFilter::JPXDecode]
                    | [StreamFilter::FlateDecode(_)]
                    | [StreamFilter::JBIG2Decode(_)] => Ok((data, Some(&image_filters[0]))),
                    _ => bail!("??? filters={:?}", image_filters),
                }
            }
        }
    }

    pub fn image_data(&self, resolve: &impl Resolve) -> Result<Arc<[u8]>> {
        let (data, filter) = self.raw_image_data(resolve)?;
        let filter = match filter {
            Some(f) => f,
            None => return Ok(data),
        };
        let mut data = match filter {
            StreamFilter::CCITTFaxDecode(ref params) => {
                if self.inner.info.width != params.columns {
                    bail!(
                        "image width mismatch {} != {}",
                        self.inner.info.width,
                        params.columns
                    );
                }
                let mut data = fax_decode(&data, params)?;
                if params.rows == 0 {
                    // adjust size
                    data.truncate(self.inner.info.height as usize * self.inner.info.width as usize);
                }
                data
            }
            StreamFilter::DCTDecode(ref p) => dct_decode(&data, p)?,
            StreamFilter::JPXDecode => jpx_decode(&data)?,
            StreamFilter::JBIG2Decode(ref p) => {
                let global_data = p.globals.as_ref().map(|s| s.data(resolve)).transpose()?;
                jbig2_decode(&data, global_data.as_deref().unwrap_or_default())?
            }
            StreamFilter::FlateDecode(ref p) => flate_decode(&data, p)?,
            _ => unreachable!(),
        };
        if let Some(ref decode) = self.decode {
            if decode == &[1.0, 0.0] && self.bits_per_component == Some(1) {
                data.iter_mut().for_each(|b| *b = !*b);
            }
        }
        Ok(data.into())
    }
}

#[derive(Object, Debug, DataSize, DeepClone, ObjectWrite)]
#[pdf(Type = "XObject", Subtype = "PS")]
pub struct PostScriptDict {
    // TODO
    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, Debug, Clone, DataSize, DeepClone, ObjectWrite, Default)]
#[pdf(Type = "XObject?", Subtype = "Image")]
/// A variant of XObject
pub struct ImageDict {
    #[pdf(key = "Width")]
    pub width: u32,
    #[pdf(key = "Height")]
    pub height: u32,

    #[pdf(key = "ColorSpace")]
    pub color_space: Option<ColorSpace>,

    #[pdf(key = "BitsPerComponent")]
    pub bits_per_component: Option<i32>,
    // Note: only allowed values are 1, 2, 4, 8, 16. Enum?
    #[pdf(key = "Intent")]
    pub intent: Option<RenderingIntent>,
    // Note: default: "the current rendering intent in the graphics state" - I don't think this
    // ought to have a default then
    #[pdf(key = "ImageMask", default = "false")]
    pub image_mask: bool,

    // Mask: stream or array
    #[pdf(key = "Mask")]
    pub mask: Option<Primitive>,
    //
    /// Describes how to map image samples into the range of values appropriate for the image’s color space.
    /// If `image_mask`: either [0 1] or [1 0]. Else, the length must be twice the number of color
    /// components required by `color_space` (key ColorSpace)
    // (see Decode arrays page 344)
    #[pdf(key = "Decode")]
    pub decode: Option<Vec<f32>>,

    #[pdf(key = "Interpolate", default = "false")]
    pub interpolate: bool,

    // Alternates: Vec<AlternateImage>

    // SMask (soft mask): stream
    // SMaskInData: i32
    ///The integer key of the image’s entry in the structural parent tree
    #[pdf(key = "StructParent")]
    pub struct_parent: Option<i32>,

    #[pdf(key = "ID")]
    pub id: Option<PdfString>,

    #[pdf(key = "SMask")]
    pub smask: Option<Ref<Stream<ImageDict>>>,

    // OPI: dict
    // Metadata: stream
    // OC: dict
    #[pdf(other)]
    pub other: Dictionary,
}
