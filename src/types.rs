use object::{Object, Ref, FromPrimitive, Resolve, FromDict};
use primitive::{Primitive, PdfString, Dictionary};
use std::io;
use err::*;

// Pages:

/// Node in a page tree - type is either `Page` or `PageTree`
#[derive(Debug)]
pub enum PagesNode {
    Tree (PageTree),
    Leaf (Page),
}
impl Object for PagesNode {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        match *self {
            PagesNode::Tree (ref t) => t.serialize(out),
            PagesNode::Leaf (ref l) => l.serialize(out),
        }
    }
}

impl FromPrimitive for PagesNode {
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<PagesNode> {
        let dict = p.as_dictionary(r)?;
        Ok(
        match dict["Type"].clone().as_name()?.as_str() {
            "Page" => PagesNode::Leaf (Page::from_dict(dict, r)?),
            "Pages" => PagesNode::Tree (PageTree::from_dict(dict, r)?),
            other => bail!(ErrorKind::WrongDictionaryType {expected: "Page or Pages".into(), found: other.into()}),
        }
        )
    }
}



#[derive(FromDict, Object, Default)]
pub struct Catalog {
    #[pdf(key="Pages")]
    pub pages:  PageTree,
    
    //#[pdf(key="Labels")]
    //labels: HashMap<usize, PageLabel>
}




#[derive(Object, FromDict, Debug, Default)]
#[pdf(Type = "Pages")]
pub struct PageTree {
    #[pdf(key="Parent", opt=true)]
    pub parent: Option<Ref<PageTree>>,
    #[pdf(key="Kids", opt=false)]
    pub kids:   Vec<PagesNode>,
    #[pdf(key="Count", opt=false)]
    pub count:  i32,

    // #[pdf(key="Resources", opt=false]
    // resources: Option<Ref<Resources>>,
}
impl PageTree {
    pub fn root() -> PageTree {
        PageTree {
            parent: None,
            kids:   Vec::new(),
            count:  0
        }
    }
}

#[derive(Object, FromDict, Debug)]
pub struct Page {
    #[pdf(key="Parent", opt=false)]
    pub parent: Ref<PageTree>,
    
    //#[pdf(key="Parent", opt=true)]
    //pub ressources: Option<Ressources>,
    
    #[pdf(key="MediaBox", opt=true)]
    pub media_box:  Option<Rect>,
    
    #[pdf(key="CropBox", opt=true)]
    pub crop_box:   Option<Rect>,
    
    #[pdf(key="TrimBox", opt=true)]
    pub trim_box:   Option<Rect>,
    
    //#[pdf(key="Contents", opt=true)]
    //pub contents:   Option<PlainRef>
}
impl Page {
    pub fn new(parent: Ref<PageTree>) -> Page {
        Page {
            parent:     parent,
            media_box:  None,
            crop_box:   None,
            trim_box:   None
        }
    }
}

#[derive(Object)]
pub struct PageLabel {
    #[pdf(key="S", opt=true)]
    style:  Option<Counter>,
    
    #[pdf(key="P", opt=true)]
    prefix: Option<PdfString>,
    
    #[pdf(key="St", opt=true)]
    start:  Option<usize>
}

pub enum Counter {
    Arabic,
    RomanUpper,
    RomanLower,
    AlphaUpper,
    AlphaLower
}
impl Object for Counter {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        let style_code = match *self {
            Counter::Arabic     => "D",
            Counter::RomanLower => "r",
            Counter::RomanUpper => "R",
            Counter::AlphaLower => "a",
            Counter::AlphaUpper => "A"
        };
        out.write(style_code.as_bytes())?;
        Ok(())
    }
}



pub enum NameTreeNode<T> {
    ///
    Intermediate (Vec<Ref<NameTree<T>>>),
    ///
    Leaf (Vec<(String, T)>)

}
/// Note: The PDF concept of 'root' node is an intermediate or leaf node which has no 'Limits'
/// entry. Hence, `limits`
pub struct NameTree<T> {
    limits: (PdfString, PdfString),
    node: NameTreeNode<T>,
}

impl<T> FromDict for NameTree<T> {
    fn from_dict(dict: Dictionary, resolve: &Resolve) -> Result<Self> {
        unimplemented!(); // TODO
    }
}
impl<T> FromPrimitive for NameTree<T> {
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        unimplemented!(); // TODO
    }
}
impl<T> Object for NameTree<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        unimplemented!(); // TODO
    }
}




/// There is one NameDictionary associated with each PDF file.
#[derive(Object, FromDict)]
pub struct NameDictionary {
    /*
    #[pdf(key="Dests", opt=true)]
    ap: NameTree<T>,
    #[pdf(key="AP", opt=true)]
    ap: NameTree<T>,
    #[pdf(key="JavaScript", opt=true)]
    javascript: NameTree<T>,
    #[pdf(key="Pages", opt=true)]
    pages: NameTree<T>,
    #[pdf(key="Templates", opt=true)]
    templates: NameTree<T>,
    #[pdf(key="IDS", opt=true)]
    ids: NameTree<T>,
    #[pdf(key="URLS", opt=true)]
    urls: NameTree<T>,
    */
    #[pdf(key="EmbeddedFiles", opt=true)]
    embedded_files: Option<NameTree<FileSpecification>>,
    /*
    #[pdf(key="AlternativePresentations", opt=true)]
    alternative_presentations: NameTree<T>,
    #[pdf(key="Renditions", opt=true)]
    renditions: NameTree<T>,
    */
}

/* Embedded file streams can be associated with the document as a whole through
 * the EmbeddedFiles entry (PDF 1.4) in the PDF document’s name dictionary
 * (see Section 3.6.3, “Name Dictionary”).
 * The associated name tree maps name strings to file specifications that refer
 * to embedded file streams through their EF entries.
*/

#[derive(Object, FromDict)]
pub struct FileSpecification {
    #[pdf(key="EF", opt=true)]
    ef: Option<Files<EmbeddedFile>>,
    /*
    #[pdf(key="RF", opt=true)]
    rf: Option<Files<RelatedFilesArray>>,
    */
}

/// Used only as elements in FileSpecification
#[derive(Object, FromDict)]
pub struct Files<T: Object + FromPrimitive> {
    #[pdf(key="F", opt=true)]
    f: Option<T>,
    #[pdf(key="UF", opt=true)]
    uf: Option<T>,
    #[pdf(key="DOS", opt=true)]
    dos: Option<T>,
    #[pdf(key="Mac", opt=true)]
    mac: Option<T>,
    #[pdf(key="Unix", opt=true)]
    unix: Option<T>,
}

/// PDF Embedded File Stream.
#[derive(Object, FromDict)]
pub struct EmbeddedFile {
    /*
    #[pdf(key="Subtype", opt=true)]
    subtype: Option<String>,
    */
    #[pdf(key="Params", opt=true)]
    params: Option<EmbeddedFileParamDict>,
}

#[derive(Object, FromDict)]
pub struct EmbeddedFileParamDict {
    #[pdf(key="Size", opt=true)]
    size: Option<i32>,
    /*
    // TODO need Date type
    #[pdf(key="CreationDate", opt=true)]
    creationdate: T,
    #[pdf(key="ModDate", opt=true)]
    moddate: T,
    #[pdf(key="Mac", opt=true)]
    mac: T,
    #[pdf(key="CheckSum", opt=true)]
    checksum: T,
    */
}





#[derive(Debug)]
pub enum StreamFilter {
    AsciiHex,
    Ascii85,
    Lzw,
    Flate,
    Jpeg2k
}
impl Object for StreamFilter {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        let s = match self {
            &StreamFilter::AsciiHex => "/ASCIIHexDecode",
            &StreamFilter::Ascii85 => "/ASCII85Decode",
            &StreamFilter::Lzw => "/LZWDecode",
            &StreamFilter::Flate => "/FlateDecode",
            &StreamFilter::Jpeg2k => "/JPXDecode"
        };
        write!(out, "{}", s)
    }
}
impl FromPrimitive for StreamFilter {
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        match &p.as_name()? as &str {
            "ASCIIHexDecode"    => Ok(StreamFilter::AsciiHex),
            "ASCII85Decode"     => Ok(StreamFilter::Ascii85),
            "LZWDecode"         => Ok(StreamFilter::Lzw),
            "FlateDecode"       => Ok(StreamFilter::Flate),
            "JPXDecode"         => Ok(StreamFilter::Jpeg2k),
            _                   => Err("Filter not recognized".into()),
        }
    }
}


pub fn write_list<W, T, I>(out: &mut W, mut iter: I) -> io::Result<()>
    where W: io::Write, T: Object, I: Iterator<Item=T>
{
    write!(out, "[")?;
    
    if let Some(first) = iter.next() {
        first.serialize(out)?;
        
        for other in iter {
            out.write(b", ")?;
            other.serialize(out)?;
        }
    }
    
    write!(out, "]")
}

#[derive(Object)]
pub struct Outlines {
    #[pdf(key="Count")]
    pub count:  usize
}

#[derive(Debug)]
pub struct Rect {
    pub left:   f32,
    pub right:  f32,
    pub top:    f32,
    pub bottom: f32
}
impl FromPrimitive for Rect {
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        let arr = p.as_array(r)?;
        if arr.len() != 4 {
            bail!("len != 4");
        }
        Ok(Rect {
            left:   arr[0].as_number()?,
            right:  arr[1].as_number()?,
            top:    arr[2].as_number()?,
            bottom: arr[3].as_number()?
        })
    }
}
impl Object for Rect {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "[{} {} {} {}]", self.left, self.top, self.right, self.bottom)
    }
}
