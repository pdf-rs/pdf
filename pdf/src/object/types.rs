//! Models of PDF types

use std::io;
use std::rc::Rc;
use std::collections::HashMap;

use crate as pdf;
use crate::object::*;
use crate::error::*;
use crate::content::Content;
use crate::font::Font;
use crate::file::File;
use crate::backend::Backend;

/// Node in a page tree - type is either `Page` or `PageTree`
#[derive(Debug)]
pub enum PagesNode {
    Tree(Rc<PageTree>),
    Leaf(Rc<Page>),
}
impl Object for PagesNode {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        match *self {
            PagesNode::Tree (ref t) => t.serialize(out),
            PagesNode::Leaf (ref l) => l.serialize(out),
        }
    }
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<PagesNode> {
        let dict = Dictionary::from_primitive(p, r)?;
        match dict["Type"].clone().into_name()?.as_str() {
            "Page" => Ok(PagesNode::Leaf (Object::from_primitive(Primitive::Dictionary(dict), r)?)),
            "Pages" => Ok(PagesNode::Tree (Object::from_primitive(Primitive::Dictionary(dict), r)?)),
            other => Err(PdfError::WrongDictionaryType {expected: "Page or Pages".into(), found: other.into()}),
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

pub type PageRc = Rc<Page>;


#[derive(Object, Debug)]
pub struct Catalog {
// Version: Name,
    #[pdf(key="Pages")]
    pub pages: Rc<PagesNode>,

// PageLabels: number_tree,
    #[pdf(key="Names")]
    pub names: Option<NameDictionary>,
    
    #[pdf(key="Dests")]
    pub dests: Option<Ref<Dictionary>>,

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
// Metadata: stream
    #[pdf(key="Metadata")]
    pub metadata: Option<Stream>,

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

#[derive(Object, Debug, Default)]
#[pdf(Type = "Pages")]
pub struct PageTree {
    #[pdf(key="Parent")]
    pub parent: Option<Ref<PagesNode>>,
    #[pdf(key="Kids")]
    pub kids:   Vec<Ref<PagesNode>>,
    #[pdf(key="Count")]
    pub count:  u32,

    /// Exists to be inherited to a 'Page' object. Note: *Inheritable*.
    // Note about inheritance..= if we wanted to 'inherit' things at the time of reading, we would
    // want Option<Ref<Resources>> here most likely.
    #[pdf(key="Resources")]
    pub resources: Option<Rc<Resources>>,
    
    #[pdf(key="MediaBox")]
    pub media_box:  Option<Rect>,
    
    #[pdf(key="CropBox")]
    pub crop_box:   Option<Rect>,
}
impl PageTree {
    pub fn page(&self, resolve: &impl Resolve, page_nr: u32) -> Result<PageRc> {
        let mut pos = 0;
        for &kid in &self.kids {
            let rc = resolve.get(kid)?;
            match *rc {
                PagesNode::Tree(ref tree) => {
                    if (pos .. pos + tree.count).contains(&page_nr) {
                        return tree.page(resolve, page_nr - pos);
                    }
                    pos += tree.count;
                }
                PagesNode::Leaf(ref page) => {
                    if pos == page_nr {
                        return Ok(page.clone());
                    }
                    pos += 1;
                }
            }
        }
        Err(PdfError::PageOutOfBounds {page_nr, max: pos})
    }
    /*
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
#[derive(Object, Debug)]
pub struct Page {
    #[pdf(key="Parent")]
    pub parent: Ref<PagesNode>,

    #[pdf(key="Resources")]
    pub resources: Option<Rc<Resources>>,
    
    #[pdf(key="MediaBox")]
    pub media_box:  Option<Rect>,
    
    #[pdf(key="CropBox")]
    pub crop_box:   Option<Rect>,
    
    #[pdf(key="TrimBox")]
    pub trim_box:   Option<Rect>,
    
    #[pdf(key="Contents")]
    pub contents:   Option<Content>
}
fn inherit<T, F, B: Backend>(mut parent: Ref<PagesNode>, file: &File<B>, f: F) -> Result<Option<T>>
    where F: Fn(&PageTree) -> Option<T>
{
    while let PagesNode::Tree(ref page_tree) = *file.get(parent)? {
        debug!("parent: {:?}", page_tree);
        match (page_tree.parent, f(&page_tree)) {
            (_, Some(t)) => return Ok(Some(t)),
            (Some(ref p), None) => parent = *p,
            (None, None) => return Ok(None)
        }
    }
    bail!("bad parent")
}

impl Page {
    pub fn new(parent: Ref<PagesNode>) -> Page {
        Page {
            parent,
            media_box:  None,
            crop_box:   None,
            trim_box:   None,
            resources:  None,
            contents:   None
        }
    }
    pub fn media_box<B: Backend>(&self, file: &File<B>) -> Result<Rect> {
        match self.media_box {
            Some(b) => Ok(b),
            None => inherit(self.parent, file, |pt| pt.media_box)?
                .ok_or_else(|| PdfError::MissingEntry { typ: "Page", field: "MediaBox".into() })
        }
    }
    pub fn crop_box<B: Backend>(&self, file: &File<B>) -> Result<Rect> {
        match self.crop_box {
            Some(b) => Ok(b),
            None => match inherit(self.parent, file, |pt| pt.crop_box)? {
                Some(b) => Ok(b),
                None => self.media_box(file)
            }
        }
    }
    pub fn resources<B: Backend>(&self, file: &File<B>) -> Result<Rc<Resources>> {
        match self.resources {
            Some(ref r) => Ok(r.clone()),
            None => inherit(self.parent, file, |pt| pt.resources.clone())?
                .ok_or_else(|| PdfError::MissingEntry { typ: "Page", field: "Resources".into() })
        }
    }
}

#[derive(Object)]
pub struct PageLabel {
    #[pdf(key="S")]
    style:  Option<Counter>,
    
    #[pdf(key="P")]
    prefix: Option<PdfString>,
    
    #[pdf(key="St")]
    start:  Option<usize>
}

#[derive(Object, Debug)]
pub struct Resources {
    #[pdf(key="ExtGState")]
    pub graphics_states: HashMap<String, GraphicsStateParameters>,

    #[pdf(key="ColorSpace")]
    pub color_spaces: HashMap<String, ColorSpace>,

    // pattern: Option<Pattern>,
    // shading: Option<Shading>,
    #[pdf(key="XObject")]
    pub xobjects: HashMap<String, Ref<XObject>>,
    // /XObject is a dictionary that map arbitrary names to XObjects
    #[pdf(key="Font")]
    pub fonts: HashMap<String, Rc<Font>>,
}
impl Resources {
    pub fn fonts(&self) -> impl Iterator<Item=(&str, &Rc<Font>)> {
        self.fonts.iter().map(|(k, v)| (k.as_str(), v))
    }
}


#[derive(Object, Debug)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2
}
#[derive(Object, Debug)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2
}

#[derive(Object, Debug)]
#[pdf(Type = "ExtGState?")]
/// `ExtGState`
pub struct GraphicsStateParameters {
    #[pdf(key="LW")]
    pub line_width: Option<f32>,
    
    #[pdf(key="LC")]
    pub line_cap: Option<LineCap>,
    
    #[pdf(key="LC")]
    pub line_join: Option<LineJoin>,
    
    #[pdf(key="ML")]
    pub miter_limit: Option<f32>,
    
    // D : dash pattern
    #[pdf(key="RI")]
    pub rendering_intent: Option<String>,
    
    #[pdf(key="Font")]
    pub font: Option<(Rc<Font>, f32)>,

    #[pdf(other)]
    _other: Dictionary
}

#[derive(Object, Debug)]
#[pdf(is_stream)]
pub enum XObject {
    #[pdf(name="PS")]
    Postscript (PostScriptXObject),
    Image (ImageXObject),
    Form (FormXObject),
}

/// A variant of XObject
pub type PostScriptXObject = Rc<Stream<PostScriptDict>>;
/// A variant of XObject
pub type ImageXObject = Rc<Stream<ImageDict>>;
/// A variant of XObject
pub type FormXObject = Rc<Stream<FormDict>>;

#[derive(Object, Debug)]
#[pdf(Type="XObject", Subtype="PS")]
pub struct PostScriptDict {
    // TODO
}

#[derive(Object, Debug)]
#[pdf(Type="XObject?", Subtype="Image")]
/// A variant of XObject
pub struct ImageDict {
    #[pdf(key="Width")]
    pub width: i32,
    #[pdf(key="Height")]
    pub height: i32,

    #[pdf(key="ColorSpace")]
    pub color_space: Option<Primitive>,

    #[pdf(key="BitsPerComponent")]
    pub bits_per_component: i32,
    // Note: only allowed values are 1, 2, 4, 8, 16. Enum?
    
    #[pdf(key="Intent")]
    pub intent: Option<RenderingIntent>,
    // Note: default: "the current rendering intent in the graphics state" - I don't think this
    // ought to have a default then

    #[pdf(key="ImageMask", default="false")]
    pub image_mask: bool,

    // Mask: stream or array
    #[pdf(key="Mask")]
    pub mask: Primitive,
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

    // OPI: dict
    // Metadata: stream
    // OC: dict
    
    #[pdf(other)]
    other: Dictionary
}


#[derive(Object, Debug, Clone)]
pub enum RenderingIntent {
    AbsoluteColorimetric,
    RelativeColorimetric,
    Saturation,
    Perceptual,
}


#[derive(Object, Debug)]
#[pdf(Type="XObject?", Subtype="Form")]
pub struct FormDict {
    // TODO
}


pub enum Counter {
    Arabic,
    RomanUpper,
    RomanLower,
    AlphaUpper,
    AlphaLower
}
impl Object for Counter {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        let style_code = match *self {
            Counter::Arabic     => "D",
            Counter::RomanLower => "r",
            Counter::RomanUpper => "R",
            Counter::AlphaLower => "a",
            Counter::AlphaUpper => "A"
        };
        out.write_all(style_code.as_bytes())?;
        Ok(())
    }
    fn from_primitive(_: Primitive, _: &impl Resolve) -> Result<Self> {
        unimplemented!();
    }
}

#[derive(Debug)]
pub enum NameTreeNode<T> {
    ///
    Intermediate (Vec<Ref<NameTree<T>>>),
    ///
    Leaf (Vec<(PdfString, T)>)

}
/// Note: The PDF concept of 'root' node is an intermediate or leaf node which has no 'Limits'
/// entry. Hence, `limits`, 
#[derive(Debug)]
pub struct NameTree<T> {
    pub limits: Option<(PdfString, PdfString)>,
    pub node: NameTreeNode<T>,
}
impl<T: Object> NameTree<T> {
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
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut dict = p.into_dictionary(resolve)?;
        
        // Quite long function..=
        let limits = match dict.remove("Limits") {
            Some(limits) => {
                let limits = limits.into_array(resolve)?;
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
                let kids = kids.into_array(resolve)?.iter().map(|kid|
                    Ref::<NameTree<T>>::from_primitive(kid.clone(), resolve)
                ).collect::<Result<Vec<_>>>()?;
                NameTree {
                    limits,
                    node: NameTreeNode::Intermediate (kids)
                }
            }
            (None, Some(names)) => {
                let names = names.into_array(resolve)?;
                let mut new_names = Vec::new();
                for pair in names.chunks(2) {
                    let name = pair[0].clone().into_string()?;
                    let value = T::from_primitive(pair[1].clone(), resolve)?;
                    new_names.push((name, value));
                }
                NameTree {
                    limits,
                    node: NameTreeNode::Leaf (new_names),
                }
            }
            (None, None) => bail!("Neither Kids nor Names present in NameTree node.")
        })
    }
}

#[derive(Debug, Clone)]
pub enum DestView {
    // left, top, zoom
    XYZ { left: Option<f32>, top: Option<f32>, zoom: f32 },
    Fit,
    FitH { top: f32 },
    FitV { left: f32 },
    FitR(Rect),
    FitB,
    FitBH { top: f32 }
}

#[derive(Debug, Clone)]
pub struct Dest {
    pub page: Ref<PagesNode>,
    pub view: DestView
}
impl Object for Dest {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let p = match p {
            Primitive::Reference(r) => resolve.resolve(r)?,
            p => p
        };
        let p = match p {
            Primitive::Dictionary(mut dict) => dict.require("Dest", "D")?,
            p => p
        };
        let array = p.as_array()?;
        let page = Ref::from_primitive(try_opt!(array.get(0)).clone(), resolve)?;
        let kind = try_opt!(array.get(1));
        let view = match kind.as_name()? {
            "XYZ" => DestView::XYZ {
                left: match try_opt!(array.get(2)) {
                    &Primitive::Null => None,
                    &Primitive::Integer(n) => Some(n as f32),
                    &Primitive::Number(f) => Some(f),
                    ref p => return Err(PdfError::UnexpectedPrimitive { expected: "Number | Integer | Null", found: p.get_debug_name() }),
                },
                top: match try_opt!(array.get(3)) {
                    &Primitive::Null => None,
                    &Primitive::Integer(n) => Some(n as f32),
                    &Primitive::Number(f) => Some(f),
                    ref p => return Err(PdfError::UnexpectedPrimitive { expected: "Number | Integer | Null", found: p.get_debug_name() }),
                },
                zoom: match try_opt!(array.get(4)) {
                    &Primitive::Null => 0.0,
                    &Primitive::Integer(n) => n as f32,
                    &Primitive::Number(f) => f,
                    ref p => return Err(PdfError::UnexpectedPrimitive { expected: "Number | Integer | Null", found: p.get_debug_name() }),
                },
            },
            "Fit" => DestView::Fit,
            "FitH" => DestView::FitH {
                top: try_opt!(array.get(2)).as_number()?
            },
            "FitV" => DestView::FitV {
                left: try_opt!(array.get(2)).as_number()?
            },
            "FitR" => DestView::FitR(Rect {
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

/// There is one `NameDictionary` associated with each PDF file.
#[derive(Object, Debug)]
pub struct NameDictionary {
    #[pdf(key="Pages")]
    pub pages: Option<NameTree<Primitive>>,
    
    #[pdf(key="Dests")]
    pub dests: Option<NameTree<Dest>>,
    
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

#[derive(Object, Debug, Clone)]
pub struct FileSpec {
    #[pdf(key="EF")]
    ef: Option<Files<Ref<Stream<EmbeddedFile>>>>,
    /*
    #[pdf(key="RF")]
    rf: Option<Files<RelatedFilesArray>>,
    */
}

/// Used only as elements in `FileSpec`
#[derive(Object, Debug, Clone)]
pub struct Files<T: Object> {
    #[pdf(key="F")]
    f: Option<T>,
    #[pdf(key="UF")]
    uf: Option<T>,
    #[pdf(key="DOS")]
    dos: Option<T>,
    #[pdf(key="Mac")]
    mac: Option<T>,
    #[pdf(key="Unix")]
    unix: Option<T>,
}

/// PDF Embedded File Stream.
#[derive(Object, Debug, Clone)]
pub struct EmbeddedFile {
    /*
    #[pdf(key="Subtype")]
    subtype: Option<String>,
    */
    #[pdf(key="Params")]
    params: Option<EmbeddedFileParamDict>,
}

#[derive(Object, Debug, Clone)]
pub struct EmbeddedFileParamDict {
    #[pdf(key="Size")]
    size: Option<i32>,
    /*
    // TODO need Date type
    #[pdf(key="CreationDate")]
    creationdate: T,
    #[pdf(key="ModDate")]
    moddate: T,
    #[pdf(key="Mac")]
    mac: T,
    #[pdf(key="CheckSum")]
    checksum: T,
    */
}






pub fn write_list<'a, W, T: 'a, I>(out: &mut W, mut iter: I) -> Result<()>
    where W: io::Write, T: Object, I: Iterator<Item=&'a T>
{
    write!(out, "[")?;
    
    if let Some(first) = iter.next() {
        first.serialize(out)?;
        
        for other in iter {
            out.write_all(b", ")?;
            other.serialize(out)?;
        }
    }
    
    write!(out, "]")?;
    Ok(())
}

#[derive(Object, Debug, Clone)]
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
    pub dest: Option<PdfString>,

    #[pdf(key="A")]
    pub action: Option<Dictionary>,

    #[pdf(key="SE")]
    pub se: Option<Dictionary>,

    #[pdf(key="C")]
    pub color: Option<Vec<f32>>,

    #[pdf(key="F")]
    pub flags: Option<i32>,
}

#[derive(Object, Debug)]
#[pdf(Type="Outlines?")]
pub struct Outlines {
    #[pdf(key="Count", default="0")]
    pub count:  i32,

    #[pdf(key="First")]
    pub first: Option<Ref<OutlineItem>>,

    #[pdf(key="Last")]
    pub last: Option<Ref<OutlineItem>>,

}

#[derive(Debug, Copy, Clone)]
pub struct Rect {
    pub left:   f32,
    pub bottom: f32,
    pub right:  f32,
    pub top:    f32,
}
impl Object for Rect {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "[{} {} {} {}]", self.left, self.top, self.right, self.bottom)?;
        Ok(())
    }
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        let arr = p.into_array(r)?;
        if arr.len() != 4 {
            bail!("len != 4");
        }
        Ok(Rect {
            left:   arr[0].as_number()?,
            bottom: arr[1].as_number()?,
            right:  arr[2].as_number()?,
            top:    arr[3].as_number()?
        })
    }
}


// Stuff from chapter 10 of the PDF 1.7 ref

#[derive(Object, Debug)]
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

#[derive(Object, Debug)]
#[pdf(Type = "StructTreeRoot")]
pub struct StructTreeRoot {
    #[pdf(key="K")]
    pub children: Vec<StructElem>,
}
#[derive(Object, Debug)]
pub struct StructElem {
    #[pdf(key="S")]
    struct_type: StructType,

    #[pdf(key="P")]
    parent: Ref<StructElem>,

    #[pdf(key="ID")]
    id: Option<PdfString>,

    /// `Pg`: A page object representing a page on which some or all of the content items designated by the K entry are rendered.
    #[pdf(key="Pg")]
    page: Option<Ref<Page>>,
}

#[derive(Object, Debug)]
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

#[cfg(test)]
mod tests {
    use crate::{
        object::{NoResolve, Object, StructType},
        primitive::Primitive,
    };

    #[test]
    fn parse_struct_type() {
        assert!(matches!(
            StructType::from_primitive(Primitive::Name("BibEntry".to_string()), &NoResolve),
            Ok(StructType::BibEntry)
        ));

        let result =
            StructType::from_primitive(Primitive::Name("CustomStructType".to_string()), &NoResolve);
        if let Ok(StructType::Other(name)) = &result {
            assert_eq!(name, "CustomStructType");
        } else {
            panic!("Incorrect result of {:?}", &result);
        }
    }
}
