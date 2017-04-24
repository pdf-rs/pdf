use object::{Object, Ref, FromPrimitive, Resolve, FromDict};
use primitive::Primitive;
use std::io;
use err::*;
use std::io::Write;
use encoding::all::UTF_16BE;

/// Node in a page tree - type is either `Page` or `Pages`
#[derive(Debug)]
pub enum PagesNode {
    Tree (Pages),
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
            "Pages" => PagesNode::Tree (Pages::from_dict(dict, r)?),
            _ => bail!("Pages node points to a Dictionary but it's not of type Page or Pages."),
        }
        )
    }
}




/* Dictionary Types */

#[derive(FromDict, Object)]
pub struct Catalog {
    #[pdf(key="Pages")]
    pub pages:  Pages,
    // #[pdf(key="Labels", opt=false]
    // labels: HashMap<usize, PageLabel>
}





#[derive(Object, FromDict, Debug)]
pub struct Pages { // TODO would like to call it PageTree, but the macro would have to change
    #[pdf(key="Parent", opt=true)]
    pub parent: Option<Ref<Pages>>,
    #[pdf(key="Kids", opt=false)]
    pub kids:   Vec<PagesNode>,
    #[pdf(key="Count", opt=false)]
    pub count:  i32,

    // #[pdf(key="Resources", opt=false]
    // resources: Option<Ref<Resources>>,
}

#[derive(Object, FromDict, Debug)]
pub struct Page {
    #[pdf(key="Parent", opt=false)]
    pub parent: Ref<Pages>,
}

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
