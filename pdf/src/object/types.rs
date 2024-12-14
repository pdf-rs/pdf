//! Models of PDF types

use std::collections::HashMap;
use datasize::DataSize;

use crate as pdf;
use crate::content::deep_clone_op;
use crate::object::*;
use crate::error::*;
use crate::content::{Content, FormXObject, Matrix, parse_ops, serialize_ops, Op};
use crate::font::Font;
use crate::enc::StreamFilter;

/// Node in a page tree - type is either `Page` or `PageTree`
#[derive(Debug, Clone, DataSize)]
pub enum PagesNode {
    Tree(PageTree),
    Leaf(Page),
}

impl Object for PagesNode {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<PagesNode> {
        let mut dict = p.resolve(resolve)?.into_dictionary()?;
        match dict.require("PagesNode", "Type")?.as_name()? {
            "Page" => Ok(PagesNode::Leaf(t!(Page::from_dict(dict, resolve)))),
            "Pages" => Ok(PagesNode::Tree(t!(PageTree::from_dict(dict, resolve)))),
            other => Err(PdfError::WrongDictionaryType {expected: "Page or Pages".into(), found: other.into()}),
        }
    }
}
impl ObjectWrite for PagesNode {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match *self {
            PagesNode::Tree(ref t) => t.to_primitive(update),
            PagesNode::Leaf(ref l) => l.to_primitive(update),
        }
    }
}

/*
use std::iter::once;
use itertools::Either;
// needs recursive types
impl PagesNode {
    pub fn pages<'a>(&'a self, resolve: &'a impl Resolve) -> impl Iterator<Item=Result<PageRc>> + 'a {
        match self {
            PagesNode::Tree(ref tree) => Either::Left(Box::new(tree.pages(resolve))),
            PagesNode::Leaf(ref page) => Either::Right(once(Ok(PageRc(page.clone()))))
        }
    }
}
*/

/// A `PagesNode::Leaf` wrapped in a `RcRef`
/// 
#[derive(Debug, Clone, DataSize)]
pub struct PageRc(RcRef<PagesNode>);
impl Deref for PageRc {
    type Target = Page;
    fn deref(&self) -> &Page {
        match *self.0 {
            PagesNode::Leaf(ref page) => page,
            _ => unreachable!()
        }
    }
}
impl PageRc {
    pub fn create(page: Page, update: &mut impl Updater) -> Result<PageRc> {
        Ok(PageRc(update.create(PagesNode::Leaf(page))?))
    }
    pub fn update(page: Page, old_page: &PageRc, update: &mut impl Updater) -> Result<PageRc> {
        update.update(old_page.get_plain_ref(), PagesNode::Leaf(page)).map(PageRc)
    }
    pub fn get_ref(&self) -> Ref<PagesNode> {
        self.0.get_ref()
    }
    pub fn get_plain_ref(&self) -> PlainRef {
        self.0.inner
    }
}
impl Object for PageRc {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<PageRc> {
        let node = t!(RcRef::from_primitive(p, resolve));
        match *node {
            PagesNode::Tree(_) => Err(PdfError::WrongDictionaryType {expected: "Page".into(), found: "Pages".into()}),
            PagesNode::Leaf(_) => Ok(PageRc(node))
        }
    }
}
impl ObjectWrite for PageRc {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.0.to_primitive(update)
    }
}

/// A `PagesNode::Tree` wrapped in a `RcRef`
/// 
#[derive(Debug, Clone, DataSize)]
pub struct PagesRc(RcRef<PagesNode>);
impl Deref for PagesRc {
    type Target = PageTree;
    fn deref(&self) -> &PageTree {
        match *self.0 {
            PagesNode::Tree(ref tree) => tree,
            _ => unreachable!()
        }
    }
}
impl PagesRc {
    pub fn create(tree: PageTree, update: &mut impl Updater) -> Result<PagesRc> {
        Ok(PagesRc(update.create(PagesNode::Tree(tree))?))
    }
}
impl Object for PagesRc {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<PagesRc> {
        let node = t!(RcRef::from_primitive(p, resolve));
        match *node {
            PagesNode::Leaf(_) => Err(PdfError::WrongDictionaryType {expected: "Pages".into(), found: "Page".into()}),
            PagesNode::Tree(_) => Ok(PagesRc(node))
        }
    }
}
impl ObjectWrite for PagesRc {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.0.to_primitive(update)
    }
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type = "Catalog?")]
pub struct Catalog {
    #[pdf(key="Version")]
    pub version: Option<Name>,

    #[pdf(key="Pages")]
    pub pages: PagesRc,

    #[pdf(key="PageLabels")]
    pub page_labels: Option<NumberTree<PageLabel>>,

    #[pdf(key="Names")]
    pub names: Option<MaybeRef<NameDictionary>>,
    
    #[pdf(key="Dests")]
    pub dests: Option<MaybeRef<Dictionary>>,

// ViewerPreferences: dict
// PageLayout: name
// PageMode: name

    #[pdf(key="Outlines")]
    pub outlines: Option<Outlines>,
// Threads: array
// OpenAction: array or dict
// AA: dict
// URI: dict
// AcroForm: dict
    #[pdf(key="AcroForm")]
    pub forms: Option<InteractiveFormDictionary>,

// Metadata: stream
    #[pdf(key="Metadata")]
    pub metadata: Option<Ref<Stream<()>>>,

    #[pdf(key="StructTreeRoot")]
    pub struct_tree_root: Option<StructTreeRoot>,

// MarkInfo: dict
// Lang: text string
// SpiderInfo: dict
// OutputIntents: array
// PieceInfo: dict
// OCProperties: dict
// Perms: dict
// Legal: dict
// Requirements: array
// Collection: dict
// NeedsRendering: bool
}

#[derive(Object, ObjectWrite, Debug, Default, Clone, DataSize)]
#[pdf(Type = "Pages?")]
pub struct PageTree {
    #[pdf(key="Parent")]
    pub parent: Option<PagesRc>,

    #[pdf(key="Kids")]
    pub kids:   Vec<Ref<PagesNode>>,

    #[pdf(key="Count")]
    pub count:  u32,

    #[pdf(key="Resources")]
    pub resources: Option<MaybeRef<Resources>>,
    
    #[pdf(key="MediaBox")]
    pub media_box:  Option<Rectangle>,
    
    #[pdf(key="CropBox")]
    pub crop_box:   Option<Rectangle>,
}
impl PageTree {
    pub fn page(&self, resolve: &impl Resolve, page_nr: u32) -> Result<PageRc> {
        self.page_limited(resolve, page_nr, 16)
    }
    fn page_limited(&self, resolve: &impl Resolve, page_nr: u32, depth: usize) -> Result<PageRc> {
        if depth == 0 {
            bail!("page tree depth exeeded");
        }
        let mut pos = 0;
        for &kid in &self.kids {
            let node = resolve.get(kid)?;
            match *node {
                PagesNode::Tree(ref tree) => {
                    if (pos .. pos + tree.count).contains(&page_nr) {
                        return tree.page_limited(resolve, page_nr - pos, depth - 1);
                    }
                    pos += tree.count;
                }
                PagesNode::Leaf(ref _page) => {
                    if pos == page_nr {
                        return Ok(PageRc(node));
                    }
                    pos += 1;
                }
            }
        }
        Err(PdfError::PageOutOfBounds {page_nr, max: pos})
    }

    /*
    pub fn update_pages(&mut self, mut offset: u32, page_nr: u32, page: Page) -> Result<()> {
        for kid in &self.kids {
            // println!("{}/{} {:?}", offset, page_nr, kid);
            match *(self.get(*kid)?) {
                PagesNode::Tree(ref mut t) => {
                    if offset + t.count < page_nr {
                        offset += t.count;
                    } else {
                        return self.update_pages(t, offset, page_nr, page);
                    }
                },
                PagesNode::Leaf(ref mut p) => {
                    if offset < page_nr {
                        offset += 1;
                    } else {
                        assert_eq!(offset, page_nr);
                        let p = self.storage.create(page)?;
                        self.storage.update(kid.get_inner(), PagesNode::Leaf(p));
                        return Ok(());
                    }
                }
            }
            
        }
        Err(PdfError::PageNotFound {page_nr: page_nr})
    }
    pub fn pages<'a>(&'a self, resolve: &'a impl Resolve) -> impl Iterator<Item=Result<PageRc>> + 'a {
        self.kids.iter().flat_map(move |&r| {
            match resolve.get(r) {
                Ok(node) => Either::Left(node.pages(resolve)),
                Err(e) => Either::Right(once(Err(e)))
            }
        })
    }
    */
}
impl SubType<PagesNode> for PageTree {}

#[derive(Object, ObjectWrite, Debug, Clone, DataSize)]
#[pdf(Type="Page?")]
pub struct Page {
    #[pdf(key="Parent")]
    pub parent: PagesRc,

    #[pdf(key="Resources", indirect)]
    pub resources: Option<MaybeRef<Resources>>,
    
    #[pdf(key="MediaBox")]
    pub media_box:  Option<Rectangle>,
    
    #[pdf(key="CropBox")]
    pub crop_box:   Option<Rectangle>,
    
    #[pdf(key="TrimBox")]
    pub trim_box:   Option<Rectangle>,
    
    #[pdf(key="Contents")]
    pub contents:   Option<Content>,

    #[pdf(key="Rotate", default="0")]
    pub rotate: i32,

    #[pdf(key="Metadata")]
    pub metadata:   Option<Primitive>,

    #[pdf(key="LGIDict")]
    pub lgi:        Option<Primitive>,

    #[pdf(key="VP")]
    pub vp:         Option<Primitive>,

    #[pdf(key="Annots")]
    pub annotations: Lazy<MaybeRef<Vec<MaybeRef<Annot>>>>,

    #[pdf(other)]
    pub other: Dictionary,
}
fn inherit<'a, T: 'a, F>(mut parent: &'a PageTree, f: F) -> Result<Option<T>>
    where F: Fn(&'a PageTree) -> Option<T>
{
    loop {
        match (&parent.parent, f(parent)) {
            (_, Some(t)) => return Ok(Some(t)),
            (Some(ref p), None) => parent = p,
            (None, None) => return Ok(None)
        }
    }
}

impl Page {
    pub fn new(parent: PagesRc) -> Page {
        Page {
            parent,
            media_box:  None,
            crop_box:   None,
            trim_box:   None,
            resources:  None,
            contents:   None,
            rotate:     0,
            metadata:   None,
            lgi:        None,
            vp:         None,
            other: Dictionary::new(),
            annotations: Default::default(),
        }
    }
    pub fn media_box(&self) -> Result<Rectangle> {
        match self.media_box {
            Some(b) => Ok(b),
            None => inherit(&self.parent, |pt| pt.media_box)?
                .ok_or_else(|| PdfError::MissingEntry { typ: "Page", field: "MediaBox".into() })
        }
    }
    pub fn crop_box(&self) -> Result<Rectangle> {
        match self.crop_box {
            Some(b) => Ok(b),
            None => match inherit(&self.parent, |pt| pt.crop_box)? {
                Some(b) => Ok(b),
                None => self.media_box()
            }
        }
    }
    pub fn resources(&self) -> Result<&MaybeRef<Resources>> {
        match self.resources {
            Some(ref r) => Ok(r),
            None => inherit(&self.parent, |pt| pt.resources.as_ref())?
                .ok_or_else(|| PdfError::MissingEntry { typ: "Page", field: "Resources".into() })
        }
    }
}
impl SubType<PagesNode> for Page {}


#[derive(Object, DataSize, Debug, ObjectWrite)]
pub struct PageLabel {
    #[pdf(key="S")]
    pub style:  Option<Counter>,
    
    #[pdf(key="P")]
    pub prefix: Option<PdfString>,
    
    #[pdf(key="St")]
    pub start:  Option<usize>
}

#[derive(Object, ObjectWrite, Debug, DataSize, Default, DeepClone, Clone)]
pub struct Resources {
    #[pdf(key="ExtGState")]
    pub graphics_states: HashMap<Name, GraphicsStateParameters>,

    #[pdf(key="ColorSpace")]
    pub color_spaces: HashMap<Name, ColorSpace>,

    #[pdf(key="Pattern")]
    pub pattern: HashMap<Name, Ref<Pattern>>,

    // shading: Option<Shading>,
    #[pdf(key="XObject")]
    pub xobjects: HashMap<Name, Ref<XObject>>,
    // /XObject is a dictionary that map arbitrary names to XObjects
    #[pdf(key="Font")]
    pub fonts: HashMap<Name, MaybeRef<Font>>,

    #[pdf(key="Properties")]
    pub properties: HashMap<Name, MaybeRef<Dictionary>>,
}
impl Resources {
    pub fn fonts(&self) -> impl Iterator<Item=(&str, &MaybeRef<Font>)> {
        self.fonts.iter().map(|(k, v)| (k.as_str(), v))
    }
}


#[derive(Debug, Object, ObjectWrite, DataSize, Clone, DeepClone)]
pub struct PatternDict {
    #[pdf(key="PaintType")]
    pub paint_type: Option<i32>,

    #[pdf(key="TilingType")]
    pub tiling_type: Option<i32>,

    #[pdf(key="BBox")]
    pub bbox: Rectangle,

    #[pdf(key="XStep")]
    pub x_step: f32,

    #[pdf(key="YStep")]
    pub y_step: f32,

    #[pdf(key="Resources")]
    pub resources: Ref<Resources>,

    #[pdf(key="Matrix")]
    pub matrix: Option<Matrix>,
}

#[derive(Debug, DataSize)]
pub enum Pattern {
    Dict(PatternDict),
    Stream(PatternDict, Vec<Op>),
}
impl Pattern {
    pub fn dict(&self) -> &PatternDict {
        match *self {
            Pattern::Dict(ref d) => d,
            Pattern::Stream(ref d, _) => d,
        }
    }
}
impl Object for Pattern {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let p = p.resolve(resolve)?;
        match p {
            Primitive::Dictionary(dict) => Ok(Pattern::Dict(PatternDict::from_dict(dict, resolve)?)),
            Primitive::Stream(s) => {
                let stream: Stream<PatternDict> = Stream::from_stream(s, resolve)?;
                let data = stream.data(resolve)?;
                let ops = t!(parse_ops(&data, resolve));
                let dict = stream.info.info;
                Ok(Pattern::Stream(dict, ops))
            }
            p => Err(PdfError::UnexpectedPrimitive { expected: "Dictionary or Stream", found: p.get_debug_name() })
        }
    }
}
impl ObjectWrite for Pattern {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            Pattern::Dict(ref d) => d.to_primitive(update),
            Pattern::Stream(ref d, ref ops) => {
                let data = serialize_ops(ops)?;
                let stream = Stream::new_with_filters(d.clone(), data, vec![]);
                stream.to_primitive(update)
            }
        }
    }
}
impl DeepClone for Pattern {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        match *self {
            Pattern::Dict(ref d) => Ok(Pattern::Dict(d.deep_clone(cloner)?)),
            Pattern::Stream(ref dict, ref ops) => {
                let old_resources = cloner.get(dict.resources)?;
                let mut resources = Resources::default();
                let ops: Vec<Op> = ops.iter().map(|op| deep_clone_op(op, cloner, &old_resources, &mut resources)).collect::<Result<Vec<_>>>()?;
                let dict = PatternDict {
                    resources: cloner.create(resources)?.get_ref(),
                    .. *dict
                };
                Ok(Pattern::Stream(dict, ops))
            }
        }
    }
}

#[derive(Object, ObjectWrite, DeepClone, Debug, DataSize, Copy, Clone)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2
}
#[derive(Object, ObjectWrite, DeepClone, Debug, DataSize, Copy, Clone)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2
}

#[derive(Object, ObjectWrite, DeepClone, Debug, DataSize, Clone)]
#[pdf(Type = "ExtGState?")]
/// `ExtGState`
pub struct GraphicsStateParameters {
    #[pdf(key="LW")]
    pub line_width: Option<f32>,
    
    #[pdf(key="LC")]
    pub line_cap: Option<LineCap>,
    
    #[pdf(key="LJ")]
    pub line_join: Option<LineJoin>,
    
    #[pdf(key="ML")]
    pub miter_limit: Option<f32>,
    
    #[pdf(key="D")]
    pub dash_pattern: Option<Vec<Primitive>>,
    
    #[pdf(key="RI")]
    pub rendering_intent: Option<Name>,

    #[pdf(key="OP")]
    pub overprint: Option<bool>,

    #[pdf(key="op")]
    pub overprint_fill: Option<bool>,

    #[pdf(key="OPM")]
    pub overprint_mode: Option<i32>,

    #[pdf(key="Font")]
    pub font: Option<(Ref<Font>, f32)>,

    // BG
    // BG2
    // UCR
    // UCR2
    // TR
    // TR2
    // HT
    // FL
    // SM
    // SA

    #[pdf(key="BM")]
    pub blend_mode: Option<Primitive>,

    #[pdf(key="SMask")]
    pub smask: Option<Primitive>,

    
    #[pdf(key="CA")]
    pub stroke_alpha: Option<f32>,

    #[pdf(key="ca")]
    pub fill_alpha: Option<f32>,

    #[pdf(key="AIS")]
    pub alpha_is_shape: Option<bool>,

    #[pdf(key="TK")]
    pub text_knockout: Option<bool>,

    #[pdf(other)]
    _other: Dictionary
}

#[derive(Object, Debug, DataSize, DeepClone)]
#[pdf(is_stream)]
pub enum XObject {
    #[pdf(name="PS")]
    Postscript (PostScriptXObject),
    Image (ImageXObject),
    Form (FormXObject),
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
    pub inner: Stream<ImageDict>
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
    Png
}

impl ImageXObject {
    pub fn from_stream(s: PdfStream, resolve: &impl Resolve) -> Result<Self> {
        let inner = Stream::from_stream(s, resolve)?;
        Ok(ImageXObject { inner })
    }

    /// Decode everything except for the final image encoding (jpeg, jbig2, jp2k, ...)
    pub fn raw_image_data(&self, resolve: &impl Resolve) -> Result<(Arc<[u8]>, Option<&StreamFilter>)> {
        match self.inner.inner_data {
            StreamData::Generated(_) => Ok((self.inner.data(resolve)?, None)),
            StreamData::Original(ref file_range, id) => {
                let filters = self.inner.filters.as_slice();
                // decode all non image filters
                let end = filters.iter().rposition(|f| match f {
                    StreamFilter::ASCIIHexDecode => false,
                    StreamFilter::ASCII85Decode => false,
                    StreamFilter::LZWDecode(_) => false,
                    StreamFilter::RunLengthDecode => false,
                    StreamFilter::Crypt => true,
                    _ => true
                }).unwrap_or(filters.len());
                
                let (normal_filters, image_filters) = filters.split_at(end);
                let data = resolve.get_data_or_decode(id, file_range.clone(), normal_filters)?;
        
                match image_filters {
                    [] => Ok((data, None)),
                    [StreamFilter::DCTDecode(_)] |
                    [StreamFilter::CCITTFaxDecode(_)] |
                    [StreamFilter::JPXDecode] |
                    [StreamFilter::FlateDecode(_)] |
                    [StreamFilter::JBIG2Decode(_)] => Ok((data, Some(&image_filters[0]))),
                    _ => bail!("??? filters={:?}", image_filters)
                }
            }
        }
    }

    pub fn image_data(&self, resolve: &impl Resolve) -> Result<Arc<[u8]>> {
        let (data, filter) = self.raw_image_data(resolve)?;
        let filter = match filter {
            Some(f) => f,
            None => return Ok(data)
        };
        let mut data = match filter {
            StreamFilter::CCITTFaxDecode(ref params) => {
                if self.inner.info.width != params.columns {
                    bail!("image width mismatch {} != {}", self.inner.info.width, params.columns);
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
            },
            StreamFilter::FlateDecode(ref p) => flate_decode(&data, p)?,
            _ => unreachable!()
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
#[pdf(Type="XObject", Subtype="PS")]
pub struct PostScriptDict {
    // TODO
    #[pdf(other)]
    pub other: Dictionary
}

#[derive(Object, Debug, Clone, DataSize, DeepClone, ObjectWrite, Default)]
#[pdf(Type="XObject?", Subtype="Image")]
/// A variant of XObject
pub struct ImageDict {
    #[pdf(key="Width")]
    pub width: u32,
    #[pdf(key="Height")]
    pub height: u32,

    #[pdf(key="ColorSpace")]
    pub color_space: Option<ColorSpace>,

    #[pdf(key="BitsPerComponent")]
    pub bits_per_component: Option<i32>,
    // Note: only allowed values are 1, 2, 4, 8, 16. Enum?
    
    #[pdf(key="Intent")]
    pub intent: Option<RenderingIntent>,
    // Note: default: "the current rendering intent in the graphics state" - I don't think this
    // ought to have a default then

    #[pdf(key="ImageMask", default="false")]
    pub image_mask: bool,

    // Mask: stream or array
    #[pdf(key="Mask")]
    pub mask: Option<Primitive>,
    //
    /// Describes how to map image samples into the range of values appropriate for the image’s color space.
    /// If `image_mask`: either [0 1] or [1 0]. Else, the length must be twice the number of color
    /// components required by `color_space` (key ColorSpace)
    // (see Decode arrays page 344)
    #[pdf(key="Decode")]
    pub decode: Option<Vec<f32>>,

    #[pdf(key="Interpolate", default="false")]
    pub interpolate: bool,

    // Alternates: Vec<AlternateImage>

    // SMask (soft mask): stream
    // SMaskInData: i32
    ///The integer key of the image’s entry in the structural parent tree
    #[pdf(key="StructParent")]
    pub struct_parent: Option<i32>,

    #[pdf(key="ID")]
    pub id: Option<PdfString>,

    #[pdf(key="SMask")]
    pub smask: Option<Ref<Stream<ImageDict>>>,

    // OPI: dict
    // Metadata: stream
    // OC: dict
    
    #[pdf(other)]
    pub other: Dictionary
}


#[derive(Object, Debug, Copy, Clone, DataSize, DeepClone, ObjectWrite)]
pub enum RenderingIntent {
    AbsoluteColorimetric,
    RelativeColorimetric,
    Saturation,
    Perceptual,
}
impl RenderingIntent {
    pub fn from_str(s: &str) -> Option<RenderingIntent> {
        match s {
            "AbsoluteColorimetric" => Some(RenderingIntent::AbsoluteColorimetric),
            "RelativeColorimetric" => Some(RenderingIntent::RelativeColorimetric),
            "Perceptual" => Some(RenderingIntent::Perceptual),
            "Saturation" => Some(RenderingIntent::Saturation),
            _ => None
        }
    }
    pub fn to_str(self) -> &'static str {
        match self {
            RenderingIntent::AbsoluteColorimetric => "AbsoluteColorimetric",
            RenderingIntent::RelativeColorimetric => "RelativeColorimetric",
            RenderingIntent::Perceptual => "Perceptual",
            RenderingIntent::Saturation => "Saturation",
        }
    }
}

#[derive(Object, Debug, DataSize, DeepClone, ObjectWrite, Clone, Default)]
#[pdf(Type="XObject?", Subtype="Form")]
pub struct FormDict {
    #[pdf(key="FormType", default="1")]
    pub form_type: i32,

    #[pdf(key="Name")]
    pub name: Option<Name>,

    #[pdf(key="LastModified")]
    pub last_modified: Option<PdfString>,

    #[pdf(key="BBox")]
    pub bbox: Rectangle,

    #[pdf(key="Matrix")]
    pub matrix: Option<Primitive>,

    #[pdf(key="Resources")]
    pub resources: Option<MaybeRef<Resources>>,

    #[pdf(key="Group")]
    pub group: Option<Dictionary>,

    #[pdf(key="Ref")]
    pub reference: Option<Dictionary>,

    #[pdf(key="Metadata")]
    pub metadata: Option<Ref<Stream<()>>>,

    #[pdf(key="PieceInfo")]
    pub piece_info: Option<Dictionary>,

    #[pdf(key="StructParent")]
    pub struct_parent: Option<i32>,

    #[pdf(key="StructParents")]
    pub struct_parents: Option<i32>,

    #[pdf(key="OPI")]
    pub opi: Option<Dictionary>,

    #[pdf(other)]
    pub other: Dictionary,
}


#[derive(Object, ObjectWrite, Debug, Clone, DataSize)]
pub struct InteractiveFormDictionary {
    #[pdf(key="Fields")]
    pub fields: Vec<RcRef<FieldDictionary>>,
    
    #[pdf(key="NeedAppearances", default="false")]
    pub need_appearences: bool,
    
    #[pdf(key="SigFlags", default="0")]
    pub sig_flags: u32,
    
    #[pdf(key="CO")]
    pub co: Option<Vec<RcRef<FieldDictionary>>>,
    
    #[pdf(key="DR")]
    pub dr: Option<MaybeRef<Resources>>,
    
    #[pdf(key="DA")]
    pub da: Option<PdfString>,
    
    #[pdf(key="Q")]
    pub q: Option<i32>,

    #[pdf(key="XFA")]
    pub xfa: Option<Primitive>,
}

#[derive(Object, ObjectWrite, Debug, Copy, Clone, PartialEq, DataSize)]
pub enum FieldType {
    #[pdf(name="Btn")]
    Button,
    #[pdf(name="Tx")]
    Text,
    #[pdf(name="Ch")]
    Choice,
    #[pdf(name="Sig")]
    Signature,
    #[pdf(name="SigRef")]
    SignatureReference,
}

#[derive(Object, ObjectWrite, Debug)]
#[pdf(Type="SV")]
pub struct SeedValueDictionary {
    #[pdf(key="Ff", default="0")]
    pub flags: u32,
    #[pdf(key="Filter")]
    pub filter: Option<Name>,
    #[pdf(key="SubFilter")]
    pub sub_filter:  Option<Vec<Name>>,
    #[pdf(key="V")]
    pub value: Option<Primitive>,
    #[pdf(key="DigestMethod")]
    pub digest_method: Vec<PdfString>,
    #[pdf(other)]
    pub other: Dictionary
}

#[derive(Object, ObjectWrite, Debug)]
#[pdf(Type="Sig?")]
pub struct SignatureDictionary {
    #[pdf(key="Filter")]
    pub filter: Name,
    #[pdf(key="SubFilter")]
    pub sub_filter: Name,
    #[pdf(key="ByteRange")]
    pub byte_range: Vec<usize>,
    #[pdf(key="Contents")]
    pub contents: PdfString,
    #[pdf(key="Cert")]
    pub cert: Vec<PdfString>,
    #[pdf(key="Reference")]
    pub reference: Option<Primitive>,
    #[pdf(key="Name")]
    pub name: Option<PdfString>,
    #[pdf(key="M")]
    pub m: Option<PdfString>,
    #[pdf(key="Location")]
    pub location: Option<PdfString>,
    #[pdf(key="Reason")]
    pub reason: Option<PdfString>,
    #[pdf(key="ContactInfo")]
    pub contact_info: Option<PdfString>,
    #[pdf(key="V")]
    pub v: i32,
    #[pdf(key="R")]
    pub r: i32,
    #[pdf(key="Prop_Build")]
    pub prop_build: Dictionary,
    #[pdf(key="Prop_AuthTime")]
    pub prop_auth_time: i32,
    #[pdf(key="Prop_AuthType")]
    pub prop_auth_type: Name,
    #[pdf(other)]
    pub other: Dictionary
}

#[derive(Object, ObjectWrite, Debug)]
#[pdf(Type="SigRef?")]
pub struct SignatureReferenceDictionary {
    #[pdf(key="TransformMethod")]
    pub transform_method: Name,
    #[pdf(key="TransformParams")]
    pub transform_params: Option<Dictionary>,
    #[pdf(key="Data")]
    pub data: Option<Primitive>,
    #[pdf(key="DigestMethod")]
    pub digest_method: Option<Name>,
    #[pdf(other)]
    pub other: Dictionary
}


#[derive(Object, ObjectWrite, Debug, Clone, DataSize)]
#[pdf(Type="Annot?")]
pub struct Annot {
    #[pdf(key="Subtype")]
    pub subtype: Name,
    
    #[pdf(key="Rect")]
    pub rect: Option<Rectangle>,

    #[pdf(key="Contents")]
    pub contents: Option<PdfString>,

    #[pdf(key="P")]
    pub page: Option<PageRc>,

    #[pdf(key="NM")]
    pub annotation_name: Option<PdfString>,

    #[pdf(key="M")]
    pub date: Option<Date>,

    #[pdf(key="F", default="0")]
    pub annot_flags: u32,

    #[pdf(key="AP")]
    pub appearance_streams: Option<MaybeRef<AppearanceStreams>>,

    #[pdf(key="AS")]
    pub appearance_state: Option<Name>,

    #[pdf(key="Border")]
    pub border: Option<Primitive>,

    #[pdf(key="C")]
    pub color: Option<Primitive>,

    #[pdf(key="InkList")]
    pub ink_list: Option<Primitive>,

    #[pdf(key="L")]
    pub line: Option<Primitive>,

    #[pdf(other)]
    pub other: Dictionary,
}

#[derive(Object, ObjectWrite, Debug, DataSize, Clone)]
pub struct FieldDictionary {
    #[pdf(key="FT")]
    pub typ: Option<FieldType>,
    
    #[pdf(key="Parent")]
    pub parent: Option<Ref<FieldDictionary>>,
    
    #[pdf(key="Kids")]
    pub kids: Vec<Ref<FieldDictionary>>,
    
    #[pdf(key="T")]
    pub name: Option<PdfString>,
    
    #[pdf(key="TU")]
    pub alt_name: Option<PdfString>,
    
    #[pdf(key="TM")]
    pub mapping_name: Option<PdfString>,
    
    #[pdf(key="Ff", default="0")]
    pub flags: u32,

    #[pdf(key="SigFlags", default="0")]
    pub sig_flags: u32,

    #[pdf(key="V")]
    pub value: Primitive,
    
    #[pdf(key="DV")]
    pub default_value: Primitive,
    
    #[pdf(key="DR")]
    pub default_resources: Option<MaybeRef<Resources>>,
    
    #[pdf(key="AA")]
    pub actions: Option<Dictionary>,

    #[pdf(key="Rect")]
    pub rect: Option<Rectangle>,

    #[pdf(key="MaxLen")]
    pub max_len: Option<u32>,

    #[pdf(key="Subtype")]
    pub subtype: Option<Name>,

    #[pdf(other)]
    pub other: Dictionary
}

#[derive(Object, ObjectWrite, Debug, DataSize, Clone, DeepClone)]
pub struct AppearanceStreams {
    #[pdf(key="N")]
    pub normal: Ref<AppearanceStreamEntry>,

    #[pdf(key="R")]
    pub rollover: Option<Ref<AppearanceStreamEntry>>,

    #[pdf(key="D")]
    pub down: Option<Ref<AppearanceStreamEntry>>,
}

#[derive(Clone, Debug, DeepClone)]
pub enum AppearanceStreamEntry {
    Single(FormXObject),
    Dict(HashMap<Name, AppearanceStreamEntry>)
}
impl Object for AppearanceStreamEntry {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p.resolve(resolve)? {
            p @ Primitive::Dictionary(_) => Object::from_primitive(p, resolve).map(AppearanceStreamEntry::Dict),
            p @ Primitive::Stream(_) => Object::from_primitive(p, resolve).map(AppearanceStreamEntry::Single),
            p => Err(PdfError::UnexpectedPrimitive {expected: "Dict or Stream", found: p.get_debug_name()})
        }
    }
}
impl ObjectWrite for AppearanceStreamEntry {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            AppearanceStreamEntry::Dict(d) => d.to_primitive(update),
            AppearanceStreamEntry::Single(s) => s.to_primitive(update),
        }
    }
}
impl DataSize for AppearanceStreamEntry {
    const IS_DYNAMIC: bool = true;
    const STATIC_HEAP_SIZE: usize = std::mem::size_of::<Self>();
    fn estimate_heap_size(&self) -> usize {
        match self {
            AppearanceStreamEntry::Dict(d) => d.estimate_heap_size(),
            AppearanceStreamEntry::Single(s) => s.estimate_heap_size()
        }
    }
}

#[derive(Debug, DataSize, Clone, Object, ObjectWrite, DeepClone)]
pub enum Counter {
    #[pdf(name="D")]
    Arabic,
    #[pdf(name="r")]
    RomanUpper,
    #[pdf(name="R")]
    RomanLower,
    #[pdf(name="a")]
    AlphaUpper,
    #[pdf(name="A")]
    AlphaLower
}

#[derive(Debug, DataSize)]
pub enum NameTreeNode<T> {
    ///
    Intermediate (Vec<Ref<NameTree<T>>>),
    ///
    Leaf (Vec<(PdfString, T)>)

}
/// Note: The PDF concept of 'root' node is an intermediate or leaf node which has no 'Limits'
/// entry. Hence, `limits`, 
#[derive(Debug, DataSize)]
pub struct NameTree<T> {
    pub limits: Option<(PdfString, PdfString)>,
    pub node: NameTreeNode<T>,
}
impl<T: Object+DataSize> NameTree<T> {
    pub fn walk(&self, r: &impl Resolve, callback: &mut dyn FnMut(&PdfString, &T)) -> Result<(), PdfError> {
        match self.node {
            NameTreeNode::Leaf(ref items) => {
                for (name, val) in items {
                    callback(name, val);
                }
            }
            NameTreeNode::Intermediate(ref items) => {
                for &tree_ref in items {
                    let tree = r.get(tree_ref)?;
                    tree.walk(r, callback)?;
                }
            }
        }
        Ok(())
    }
}

impl<T: Object> Object for NameTree<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = t!(p.resolve(resolve)?.into_dictionary());
        
        let limits = match dict.remove("Limits") {
            Some(limits) => {
                let limits = limits.resolve(resolve)?.into_array()?;
                if limits.len() != 2 {
                    bail!("Error reading NameTree: 'Limits' is not of length 2");
                }
                let min = limits[0].clone().into_string()?;
                let max = limits[1].clone().into_string()?;

                Some((min, max))
            }
            None => None
        };

        let kids = dict.remove("Kids");
        let names = dict.remove("Names");
        // If no `kids`, try `names`. Else there is an error.
        Ok(match (kids, names) {
            (Some(kids), _) => {
                let kids = t!(kids.resolve(resolve)?.into_array()?.iter().map(|kid|
                    Ref::<NameTree<T>>::from_primitive(kid.clone(), resolve)
                ).collect::<Result<Vec<_>>>());
                NameTree {
                    limits,
                    node: NameTreeNode::Intermediate (kids)
                }
            }
            (None, Some(names)) => {
                let names = names.resolve(resolve)?.into_array()?;
                let mut new_names = Vec::new();
                for pair in names.chunks_exact(2) {
                    let name = pair[0].clone().resolve(resolve)?.into_string()?;
                    let value = t!(T::from_primitive(pair[1].clone(), resolve));
                    new_names.push((name, value));
                }
                NameTree {
                    limits,
                    node: NameTreeNode::Leaf (new_names),
                }
            }
            (None, None) => {
                warn!("Neither Kids nor Names present in NameTree node.");
                NameTree {
                    limits,
                    node: NameTreeNode::Intermediate(vec![])
                }
            }
        })
    }
}

impl<T: ObjectWrite> ObjectWrite for NameTree<T> {
    fn to_primitive(&self, _update: &mut impl Updater) -> Result<Primitive> {
        todo!("impl ObjectWrite for NameTree")
    }
}

#[derive(DataSize, Debug)]
pub struct NumberTree<T> {
    pub limits: Option<(i32, i32)>,
    pub node: NumberTreeNode<T>,
}

#[derive(DataSize, Debug)]
pub enum NumberTreeNode<T> {
    Leaf(Vec<(i32, T)>),
    Intermediate(Vec<Ref<NumberTree<T>>>),
}
impl<T: Object> Object for NumberTree<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = p.resolve(resolve)?.into_dictionary()?;

        let limits = match dict.remove("Limits") {
            Some(limits) => {
                let limits = t!(limits.resolve(resolve)?.into_array());
                if limits.len() != 2 {
                    bail!("Error reading NameTree: 'Limits' is not of length 2");
                }
                let min = t!(limits[0].as_integer());
                let max = t!(limits[1].as_integer());

                Some((min, max))
            }
            None => None
        };

        let kids = dict.remove("Kids");
        let nums = dict.remove("Nums");
        match (kids, nums) {
            (Some(kids), _) => {
                let kids = t!(kids.resolve(resolve)?.into_array()?.iter().map(|kid|
                    Ref::<NumberTree<T>>::from_primitive(kid.clone(), resolve)
                ).collect::<Result<Vec<_>>>());
                Ok(NumberTree {
                    limits,
                    node: NumberTreeNode::Intermediate (kids)
                })
            }
            (None, Some(nums)) => {
                let list = nums.into_array()?;
                let mut items = Vec::with_capacity(list.len() / 2);
                for (key, item) in list.into_iter().tuples() {
                    let idx = t!(key.as_integer());
                    let val = t!(T::from_primitive(item, resolve));
                    items.push((idx, val));
                }
                Ok(NumberTree {
                    limits,
                    node: NumberTreeNode::Leaf(items)
                })
            }
            (None, None) => {
                warn!("Neither Kids nor Names present in NumberTree node.");
                Ok(NumberTree {
                    limits,
                    node: NumberTreeNode::Intermediate(vec![])
                })
            }
        }
    }
}
impl<T: ObjectWrite> ObjectWrite for NumberTree<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let mut dict = Dictionary::new();
        if let Some(limits) = self.limits {
            dict.insert("Limits", vec![limits.0.into(), limits.1.into()]);
        }
        match self.node {
            NumberTreeNode::Leaf(ref items) => {
                let mut nums = Vec::with_capacity(items.len() * 2);
                for &(idx, ref label) in items {
                    nums.push(idx.into());
                    nums.push(label.to_primitive(update)?);
                }
                dict.insert("Nums", nums);
            }
            NumberTreeNode::Intermediate(ref kids) => {
                dict.insert("Kids", kids.iter().map(|r| r.get_inner().into()).collect_vec());
            }
        }
        Ok(dict.into())
    }
}
impl<T: Object+DataSize> NumberTree<T> {
    pub fn walk(&self, r: &impl Resolve, callback: &mut dyn FnMut(i32, &T)) -> Result<(), PdfError> {
        match self.node {
            NumberTreeNode::Leaf(ref items) => {
                for &(idx, ref val) in items {
                    callback(idx, val);
                }
            }
            NumberTreeNode::Intermediate(ref items) => {
                for &tree_ref in items {
                    let tree = r.get(tree_ref)?;
                    tree.walk(r, callback)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Object, ObjectWrite, Clone, DeepClone, Debug)]
pub struct LageLabel {
    #[pdf(key="S")]
    style: Option<Counter>,
    
    #[pdf(key="P")]
    prefix: Option<PdfString>,

    #[pdf(key="St")]
    start: Option<i32>,
}

#[derive(Debug, Clone, DataSize)]
pub enum DestView {
    // left, top, zoom
    XYZ { left: Option<f32>, top: Option<f32>, zoom: f32 },
    Fit,
    FitH { top: f32 },
    FitV { left: f32 },
    FitR(Rectangle),
    FitB,
    FitBH { top: f32 }
}

#[derive(Debug, Clone, DataSize)]
pub enum MaybeNamedDest {
    Named(PdfString),
    Direct(Dest),
}

#[derive(Debug, Clone, DataSize)]
pub struct Dest {
    pub page: Option<Ref<Page>>,
    pub view: DestView
}
impl Object for Dest {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let p = match p {
            Primitive::Reference(r) => resolve.resolve(r)?,
            p => p
        };
        let p = match p {
            Primitive::Dictionary(mut dict) => dict.require("Dest", "D")?,
            p => p
        };
        let array = t!(p.as_array(), p);
        Dest::from_array(array, resolve)
    }
}
impl Dest {
    fn from_array(array: &[Primitive], resolve: &impl Resolve) -> Result<Self> {
        let page = Object::from_primitive(try_opt!(array.get(0)).clone(), resolve)?;
        let kind = try_opt!(array.get(1));
        let view = match kind.as_name()? {
            "XYZ" => DestView::XYZ {
                left: match *try_opt!(array.get(2)) {
                    Primitive::Null => None,
                    Primitive::Integer(n) => Some(n as f32),
                    Primitive::Number(f) => Some(f),
                    ref p => return Err(PdfError::UnexpectedPrimitive { expected: "Number | Integer | Null", found: p.get_debug_name() }),
                },
                top: match *try_opt!(array.get(3)) {
                    Primitive::Null => None,
                    Primitive::Integer(n) => Some(n as f32),
                    Primitive::Number(f) => Some(f),
                    ref p => return Err(PdfError::UnexpectedPrimitive { expected: "Number | Integer | Null", found: p.get_debug_name() }),
                },
                zoom: match array.get(4) {
                    Some(Primitive::Null) => 0.0,
                    Some(&Primitive::Integer(n)) => n as f32,
                    Some(&Primitive::Number(f)) => f,
                    Some(p) => return Err(PdfError::UnexpectedPrimitive { expected: "Number | Integer | Null", found: p.get_debug_name() }),
                    None => 0.0,
                },
            },
            "Fit" => DestView::Fit,
            "FitH" => DestView::FitH {
                top: try_opt!(array.get(2)).as_number()?
            },
            "FitV" => DestView::FitV {
                left: try_opt!(array.get(2)).as_number()?
            },
            "FitR" => DestView::FitR(Rectangle {
                left:   try_opt!(array.get(2)).as_number()?,
                bottom: try_opt!(array.get(3)).as_number()?,
                right:  try_opt!(array.get(4)).as_number()?,
                top:    try_opt!(array.get(5)).as_number()?,
            }),
            "FitB" => DestView::FitB,
            "FitBH" => DestView::FitBH {
                top: try_opt!(array.get(2)).as_number()?
            },
            name => return Err(PdfError::UnknownVariant { id: "Dest", name: name.into() })
        };
        Ok(Dest {
            page,
            view
        })
    }
}
impl Object for MaybeNamedDest {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let p = match p {
            Primitive::Reference(r) => resolve.resolve(r)?,
            p => p
        };
        let p = match p {
            Primitive::Dictionary(mut dict) => dict.require("Dest", "D")?,
            Primitive::String(s) => return Ok(MaybeNamedDest::Named(s)),
            p => p
        };
        let array = t!(p.as_array(), p);
        Dest::from_array(array, resolve).map(MaybeNamedDest::Direct)
    }
}
impl ObjectWrite for MaybeNamedDest {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            MaybeNamedDest::Named(s) => Ok(Primitive::String(s.clone())),
            MaybeNamedDest::Direct(d) => d.to_primitive(update)
        }
    }
}
impl ObjectWrite for Dest {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let mut arr = vec![self.page.to_primitive(update)?];
        match self.view {
            DestView::XYZ { left, top, zoom } => {
                arr.push(Primitive::Name("XYZ".into()));
                arr.push(left.to_primitive(update)?);
                arr.push(top.to_primitive(update)?);
                arr.push(Primitive::Number(zoom));
            }
            DestView::Fit => {
                arr.push(Primitive::Name("Fit".into()));
            }
            DestView::FitH { top } => {
                arr.push(Primitive::Name("FitH".into()));
                arr.push(Primitive::Number(top));
            }
            DestView::FitV { left } => {
                arr.push(Primitive::Name("FitV".into()));
                arr.push(Primitive::Number(left));
            }
            DestView::FitR(rect) => {
                arr.push(Primitive::Name("FitR".into()));
                arr.push(Primitive::Number(rect.left));
                arr.push(Primitive::Number(rect.bottom));
                arr.push(Primitive::Number(rect.right));
                arr.push(Primitive::Number(rect.top));
            }
            DestView::FitB => {
                arr.push(Primitive::Name("FitB".into()));
            }
            DestView::FitBH { top } => {
                arr.push(Primitive::Name("FitBH".into()));
                arr.push(Primitive::Number(top));
            }
        }
        Ok(Primitive::Array(arr))
    }
}

/// There is one `NameDictionary` associated with each PDF file.
#[derive(Object, ObjectWrite, Debug, DataSize)]
pub struct NameDictionary {
    #[pdf(key="Pages")]
    pub pages: Option<NameTree<Primitive>>,
    
    #[pdf(key="Dests")]
    pub dests: Option<NameTree<Option<Dest>>>,
    
    #[pdf(key="AP")]
    pub ap: Option<NameTree<Primitive>>,
    
    #[pdf(key="JavaScript")]
    pub javascript: Option<NameTree<Primitive>>,
    
    #[pdf(key="Templates")]
    pub templates: Option<NameTree<Primitive>>,
    
    #[pdf(key="IDS")]
    pub ids: Option<NameTree<Primitive>>,
    
    #[pdf(key="URLS")]
    pub urls: Option<NameTree<Primitive>>,
    
    #[pdf(key="EmbeddedFiles")]
    pub embedded_files: Option<NameTree<FileSpec>>,
    /*
    #[pdf(key="AlternativePresentations")]
    alternate_presentations: NameTree<AlternatePresentation>,
    #[pdf(key="Renditions")]
    renditions: NameTree<Rendition>,
    */
}

/* Embedded file streams can be associated with the document as a whole through
 * the EmbeddedFiles entry (PDF 1.4) in the PDF document’s name dictionary
 * (see Section 3.6.3, “Name Dictionary”).
 * The associated name tree maps name strings to file specifications that refer
 * to embedded file streams through their EF entries.
*/

#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
pub struct FileSpec {
    #[pdf(key="EF")]
    pub ef: Option<Files<Ref<Stream<EmbeddedFile>>>>,
    /*
    #[pdf(key="RF")]
    rf: Option<Files<RelatedFilesArray>>,
    */
}

/// Used only as elements in `FileSpec`
#[derive(Object, ObjectWrite, Debug, Clone, DeepClone)]
pub struct Files<T> {
    #[pdf(key="F")]
    pub f: Option<T>,
    #[pdf(key="UF")]
    pub uf: Option<T>,
    #[pdf(key="DOS")]
    pub dos: Option<T>,
    #[pdf(key="Mac")]
    pub mac: Option<T>,
    #[pdf(key="Unix")]
    pub unix: Option<T>,
}
impl<T: DataSize> DataSize for Files<T> {
    const IS_DYNAMIC: bool = T::IS_DYNAMIC;
    const STATIC_HEAP_SIZE: usize = 5 * Option::<T>::STATIC_HEAP_SIZE;

    fn estimate_heap_size(&self) -> usize {
        self.f.as_ref().map(|t| t.estimate_heap_size()).unwrap_or(0) +
        self.uf.as_ref().map(|t| t.estimate_heap_size()).unwrap_or(0) +
        self.dos.as_ref().map(|t| t.estimate_heap_size()).unwrap_or(0) +
        self.mac.as_ref().map(|t| t.estimate_heap_size()).unwrap_or(0) +
        self.unix.as_ref().map(|t| t.estimate_heap_size()).unwrap_or(0)
    }

}

/// PDF Embedded File Stream.
#[derive(Object, Debug, Clone, DataSize, DeepClone, ObjectWrite)]
pub struct EmbeddedFile {
    #[pdf(key="Subtype")]
    subtype: Option<Name>,
    
    #[pdf(key="Params")]
    pub params: Option<EmbeddedFileParamDict>,
}

#[derive(Object, Debug, Clone, DataSize, DeepClone, ObjectWrite)]
pub struct EmbeddedFileParamDict {
    #[pdf(key="Size")]
    pub size: Option<i32>,
    
    #[pdf(key="CreationDate")]
    creationdate: Option<Date>,

    #[pdf(key="ModDate")]
    moddate: Option<Date>,

    #[pdf(key="Mac")]
    mac: Option<Date>,

    #[pdf(key="CheckSum")]
    checksum: Option<PdfString>,
}

#[derive(Object, Debug, Clone, DataSize)]
pub struct OutlineItem {
    #[pdf(key="Title")]
    pub title: Option<PdfString>,

    #[pdf(key="Prev")]
    pub prev: Option<Ref<OutlineItem>>,

    #[pdf(key="Next")]
    pub next: Option<Ref<OutlineItem>>,
    
    #[pdf(key="First")]
    pub first: Option<Ref<OutlineItem>>,

    #[pdf(key="Last")]
    pub last: Option<Ref<OutlineItem>>,

    #[pdf(key="Count", default="0")]
    pub count:  i32,

    #[pdf(key="Dest")]
    pub dest: Option<Primitive>,

    #[pdf(key="A")]
    pub action: Option<Action>,

    #[pdf(key="SE")]
    pub se: Option<Dictionary>,

    #[pdf(key="C")]
    pub color: Option<Vec<f32>>,

    #[pdf(key="F")]
    pub flags: Option<i32>,
}

#[derive(Clone, Debug, DataSize)]
pub enum Action {
    Goto(MaybeNamedDest),
    Other(Dictionary)
}
impl Object for Action {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut d = t!(p.resolve(resolve)?.into_dictionary());
        let s = try_opt!(d.get("S")).as_name()?;
        match s {
            "GoTo" => {
                let dest = t!(MaybeNamedDest::from_primitive(try_opt!(d.remove("D")), resolve));
                Ok(Action::Goto(dest))
            }
            _ => Ok(Action::Other(d))
        }
    }
}
impl ObjectWrite for Action {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            Action::Goto(dest) => {
                let mut dict = Dictionary::new();
                dict.insert("D", dest.to_primitive(update)?);
                Ok(Primitive::Dictionary(dict))
            }
            Action::Other(dict) => Ok(Primitive::Dictionary(dict.clone()))
        }
    }
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type="Outlines?")]
pub struct Outlines {
    #[pdf(key="Count", default="0")]
    pub count:  i32,

    #[pdf(key="First")]
    pub first: Option<Ref<OutlineItem>>,

    #[pdf(key="Last")]
    pub last: Option<Ref<OutlineItem>>,

}

/// ISO 32000-2:2020(E) 7.9.5 Rectangles (Pg 134)
/// specifying the lower-left x, lower-left y,
/// upper-right x, and upper-right y coordinates
/// of the rectangle, in that order. The other two
/// corners of the rectangle are then assumed to
/// have coordinates (ll x , ur y ) and
/// (ur x , ll y ).
/// Also see Table 74, key BBox definition Pg 221
/// defining top, left, bottom, right labeling
#[derive(Debug, Copy, Clone, DataSize, Default)]
pub struct Rectangle {
    pub left:   f32,
    pub bottom: f32,
    pub right:  f32,
    pub top:    f32,
}
#[deprecated]
pub type Rect = Rectangle;

impl Object for Rectangle {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        let arr = p.resolve(r)?.into_array()?;
        if arr.len() != 4 {
            bail!("len != 4 {:?}", arr);
        }
        Ok(Rectangle {
            left:   arr[0].as_number()?,
            bottom: arr[1].as_number()?,
            right:  arr[2].as_number()?,
            top:    arr[3].as_number()?
        })
    }
}
impl ObjectWrite for Rectangle {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        Primitive::array::<f32, _, _, _>([self.left, self.bottom, self.right, self.top].iter(), update)
    }
}


// Stuff from chapter 10 of the PDF 1.7 ref

#[derive(Object, ObjectWrite, Debug, DataSize)]
pub struct MarkInformation { // TODO no /Type
    /// indicating whether the document conforms to Tagged PDF conventions
    #[pdf(key="Marked", default="false")]
    pub marked: bool,
    /// Indicating the presence of structure elements that contain user properties attributes
    #[pdf(key="UserProperties", default="false")]
    pub user_properties: bool, 
    /// Indicating the presence of tag suspects
    #[pdf(key="Suspects", default="false")]
    pub suspects: bool,
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type = "StructTreeRoot")]
pub struct StructTreeRoot {
    #[pdf(key="K")]
    pub children: Vec<StructElem>,
}
#[derive(Object, ObjectWrite, Debug, DataSize)]
pub struct StructElem {
    #[pdf(key="S")]
    pub struct_type: StructType,

    #[pdf(key="P")]
    pub parent: Ref<StructElem>,

    #[pdf(key="ID")]
    pub id: Option<PdfString>,

    /// `Pg`: A page object representing a page on which some or all of the content items designated by the K entry are rendered.
    #[pdf(key="Pg")]
    pub page: Option<Ref<Page>>,
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
pub enum StructType {
    Document,
    Part,
    Art,
    Sect,
    Div,
    BlockQuote,
    Caption,
    TOC,
    TOCI,
    Index,
    NonStruct,
    Private,
    Book,
    P,
    H,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    L,
    Ll,
    Lbl,
    LBody,
    Table,
    TR,
    TH,
    TD,
    THead,
    TBody,
    TFoot,
    Span,
    Quote,
    Note,
    Reference,
    BibEntry,
    Code,
    Link,
    Annot,
    Ruby,
    RB,
    RT,
    RP,
    Warichu,
    WT,
    WP,
    Figure,
    Formula,
    Form,
    #[pdf(other)]
    Other(String),
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
pub enum Trapped {
    True,
    False,
    Unknown,
}

#[derive(Object, ObjectWrite, Debug, DataSize, Default)]
pub struct InfoDict {
    #[pdf(key="Title")]
    pub title: Option<PdfString>,

    #[pdf(key="Author")]
    pub author: Option<PdfString>,

    #[pdf(key="Subject")]
    pub subject: Option<PdfString>,

    #[pdf(key="Keywords")]
    pub keywords: Option<PdfString>,

    #[pdf(key="Creator")]
    pub creator: Option<PdfString>,

    #[pdf(key="Producer")]
    pub producer: Option<PdfString>,

    #[pdf(key="CreationDate")]
    pub creation_date: Option<Date>,

    #[pdf(key="ModDate")]
    pub mod_date: Option<Date>,

    #[pdf(key="Trapped")]
    pub trapped: Option<Trapped>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_struct_type() {
        assert!(matches!(
            StructType::from_primitive(Primitive::Name("BibEntry".into()), &NoResolve),
            Ok(StructType::BibEntry)
        ));

        let result =
            StructType::from_primitive(Primitive::Name("CustomStructType".into()), &NoResolve);
        if let Ok(StructType::Other(name)) = &result {
            assert_eq!(name, "CustomStructType");
        } else {
            panic!("Incorrect result of {:?}", &result);
        }
    }

    #[test]
    fn test_field_type() {
        assert_eq!(
            FieldType::from_primitive(Primitive::Name("Tx".into()), &NoResolve).unwrap(),
            FieldType::Text
        );
    }
}
