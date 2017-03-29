use object::{Object, Ref, FromPrimitive, Resolve, MaybeRef};
use primitive::{Primitive, Dictionary};
use std::io;
use err::*;
use std::io::Write;
use encoding::all::UTF_16BE;

/// Node in a page tree - type is either `Page` or `Pages`
pub enum PagesNode {
    Tree (Ref<Pages>),
    Leaf (Ref<Page>),
}
impl Object for PagesNode {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        match *self {
            PagesNode::Tree (ref t) => t.serialize(out),
            PagesNode::Leaf (ref l) => l.serialize(out),
        }
    }
}



struct Text {
    data:   Vec<u8>
}
impl Text {
    pub fn new(s: &str) -> Text {
        use encoding::{Encoding, EncoderTrap};
        Text {
            data: UTF_16BE.encode(s, EncoderTrap::Strict).expect("encoding is broken")
        }
    }
}
impl Object for Text {
    fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write(b"(")?;
        out.write(&self.data)?;
        out.write(b")")?;
        Ok(())
    }
}
impl FromPrimitive for Text {
    fn from_primitive(p: &Primitive, _: &Resolve) -> Result<Self> {
        Ok(Text{ data: p.as_string()?.to_owned() })
    }
}




/* Dictionary Types */

#[derive(FromDict, Object)]
pub struct Root {
    #[pdf(key="Pages")]
    pages:  Ref<Pages>,
    
    #[pdf(key="Count")]
    count:  i32
    // #[pdf(key="Labels", opt=false]
    // labels: HashMap<usize, PageLabel>
}


#[derive(Object)]
pub struct Catalog {
    #[pdf(key="Pages")]
    pages:  Ref<Pages>,
    
    // #[pdf(key="Labels", opt=false]
    // labels: HashMap<usize, PageLabel>
}



#[derive(Object)]
pub struct Pages { // TODO would like to call it PageTree, but the macro would have to change
    #[pdf(key="Parent", opt=true)]
    parent: Option<Ref<Pages>>,
    #[pdf(key="Kids", opt=false)]
    kids:   Vec<PagesNode>,
    #[pdf(key="Count", opt=false)]
    count:  i32, // TODO implement Object 

    // #[pdf(key="Resources", opt=false]
    // resources: Option<Ref<Resources>>,
}

#[derive(Object)]
pub struct Page {
    #[pdf(key="Parent", opt=false)]
    parent: Ref<Pages>
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
    fn from_primitive(p: &Primitive, _: &Resolve) -> Result<Self> {
        match p.as_name()? {
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
