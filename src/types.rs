use object::{Object, Ref, Resolve, Viewer};
use primitive::{Primitive, PdfString};
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
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<PagesNode> {
        let dict = p.to_dictionary(r)?;
        Ok(
        match dict["Type"].clone().to_name()?.as_str() {
            "Page" => PagesNode::Leaf (Page::from_primitive(Primitive::Dictionary(dict), r)?),
            "Pages" => PagesNode::Tree (PageTree::from_primitive(Primitive::Dictionary(dict), r)?),
            other => bail!(ErrorKind::WrongDictionaryType {expected: "Page or Pages".into(), found: other.into()}),
        }
        )
    }
    fn view<V: Viewer>(&self, viewer: &mut V) {
        match *self {
            PagesNode::Tree (ref tree) => {
                tree.view(viewer)
            }
            PagesNode::Leaf (ref page) => {
                page.view(viewer)
            }
        }
    }
}


#[derive(Object, Default)]
pub struct Catalog {
    #[pdf(key="Pages")]
    pub pages: PageTree,

    #[pdf(key="Names", opt=true)]
    pub names: Option<NameDictionary>,
    
    //#[pdf(key="Labels")]
    //labels: HashMap<usize, PageLabel>
}




#[derive(Object, Debug, Default)]
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

#[derive(Object, Debug)]
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
        out.write_all(style_code.as_bytes())?;
        Ok(())
    }
    fn from_primitive(_: Primitive, _: &Resolve) -> Result<Self> {
        unimplemented!();
    }
    fn view<V: Viewer>(&self, viewer: &mut V) {
        // unimplemented!();
    }
}



pub enum NameTreeNode<T> {
    ///
    Intermediate (Vec<Ref<NameTree<T>>>),
    ///
    Leaf (Vec<(PdfString, T)>)

}
/// Note: The PDF concept of 'root' node is an intermediate or leaf node which has no 'Limits'
/// entry. Hence, `limits`
pub struct NameTree<T> {
    limits: Option<(PdfString, PdfString)>,
    node: NameTreeNode<T>,
}

impl<T: Object> Object for NameTree<T> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        let mut dict = p.to_dictionary(resolve).chain_err(|| "NameTree<T>")?;
        // Quite long function...
        let limits = match dict.remove("Limits") {
            Some(limits) => {
                let limits = limits.to_array(resolve)?;
                if limits.len() != 2 {
                    bail!("Error reading NameTree: 'Limits' is not of length 2");
                }
                let min = limits[0].clone().to_string()?;
                let max = limits[1].clone().to_string()?;

                Some((min, max))
            }
            None => None

        };

        let kids = dict.remove("Kids");
        let names = dict.remove("Names");
        // If no `kids`, try `names`. Else there is an error.
        Ok(match kids {
            Some(kids) => {
                let kids = kids.to_array(resolve)?.iter().map(|kid|
                    Ref::<NameTree<T>>::from_primitive(kid.clone(), resolve)
                ).collect::<Result<Vec<_>>>()?;
                NameTree {
                    limits: limits,
                    node: NameTreeNode::Intermediate (kids)
                }
            }

            None =>
                match names {
                    Some(names) => {
                        let names = names.to_array(resolve)?;
                        let mut new_names = Vec::new();
                        for pair in names.chunks(2) {
                            let name = pair[0].clone().to_string()?;
                            let value = T::from_primitive(pair[1].clone(), resolve)?;
                            new_names.push((name, value));
                        }
                        NameTree {
                            limits: limits,
                            node: NameTreeNode::Leaf (new_names),
                        }
                    }
                    None => bail!("Neither Kids nor Names present in NameTree node.")
                }
        })
    }
    fn view<V: Viewer>(&self, viewer: &mut V) {
        // unimplemented!();
    }
}




/// There is one `NameDictionary` associated with each PDF file.
#[derive(Object)]
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
    alternate_presentations: NameTree<AlternatePresentation>,
    #[pdf(key="Renditions", opt=true)]
    renditions: NameTree<Rendition>,
    */
}

/* Embedded file streams can be associated with the document as a whole through
 * the EmbeddedFiles entry (PDF 1.4) in the PDF document’s name dictionary
 * (see Section 3.6.3, “Name Dictionary”).
 * The associated name tree maps name strings to file specifications that refer
 * to embedded file streams through their EF entries.
*/

#[derive(Object)]
pub struct FileSpecification {
    #[pdf(key="EF", opt=true)]
    ef: Option<Files<EmbeddedFile>>,
    /*
    #[pdf(key="RF", opt=true)]
    rf: Option<Files<RelatedFilesArray>>,
    */
}

/// Used only as elements in `FileSpecification`
#[derive(Object)]
pub struct Files<T: Object> {
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
#[derive(Object)]
pub struct EmbeddedFile {
    /*
    #[pdf(key="Subtype", opt=true)]
    subtype: Option<String>,
    */
    #[pdf(key="Params", opt=true)]
    params: Option<EmbeddedFileParamDict>,
}

#[derive(Object)]
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
        let s = match *self {
            StreamFilter::AsciiHex => "/ASCIIHexDecode",
            StreamFilter::Ascii85 => "/ASCII85Decode",
            StreamFilter::Lzw => "/LZWDecode",
            StreamFilter::Flate => "/FlateDecode",
            StreamFilter::Jpeg2k => "/JPXDecode"
        };
        write!(out, "{}", s)
    }
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        match &p.to_name()? as &str {
            "ASCIIHexDecode"    => Ok(StreamFilter::AsciiHex),
            "ASCII85Decode"     => Ok(StreamFilter::Ascii85),
            "LZWDecode"         => Ok(StreamFilter::Lzw),
            "FlateDecode"       => Ok(StreamFilter::Flate),
            "JPXDecode"         => Ok(StreamFilter::Jpeg2k),
            _                   => Err("Filter not recognized".into()),
        }
    }
    fn view<V: Viewer>(&self, viewer: &mut V) {
        // unimplemented!();
    }
}

pub fn write_list<'a, W, T: 'a, I>(out: &mut W, mut iter: I) -> io::Result<()>
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
impl Object for Rect {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "[{} {} {} {}]", self.left, self.top, self.right, self.bottom)
    }
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        let arr = p.to_array(r)?;
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
    fn view<V: Viewer>(&self, viewer: &mut V) {
        viewer.text(format!("Rect{{{},{} to {},{}}}", self.left, self.bottom, self.right, self.top).as_str());
    }
}
