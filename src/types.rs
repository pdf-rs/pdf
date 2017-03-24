use object::{Object, PlainRef, Ref, PrimitiveConv, Resolve};
use primitive::Primitive;
use file::File;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::io;
use err::Result;
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


impl Object for str {
    fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for b in self.chars() {
            match b {
                '\\' | '(' | ')' => write!(out, r"\")?,
                c if c > '~' => panic!("only ASCII"),
                _ => ()
            }
            write!(out, "{}", b)?;
        }
        Ok(())
    }
}
impl Object for String {
    fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()> {
        (self as &str).serialize(out)
    }
}
impl PrimitiveConv for String {
    fn from_primitive(p: &Primitive, r: &Resolve) -> Result<Self> {
        Ok(p.as_name()?.to_owned())
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
impl PrimitiveConv for Text {
    fn from_primitive(p: &Primitive, r: &Resolve) -> Result<Self> {
        Ok(Text{ data: p.as_string()?.to_owned() })
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

impl<T: Object> Object for Vec<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write_list(out, self.iter())
    }
}

impl<T: PrimitiveConv> PrimitiveConv for Vec<T> {
    fn from_primitive(p: &Primitive, r: &Resolve) -> Result<Self> {
        Ok(p.as_array(r)?.iter().map(|p| T::from_primitive(p, r)).collect::<Result<Vec<T>>>()?)
    }
}
impl<T: Object> Object for [T] {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write_list(out, self.iter())
    }
}

impl Object for i32 {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }
}
impl PrimitiveConv for i32 {
    fn from_primitive(p: &Primitive, r: &Resolve) -> Result<Self> {
        p.as_integer()
    }
}

/* Dictionary Types */

#[derive(Object)]
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
impl PrimitiveConv for StreamFilter {
    fn from_primitive(p: &Primitive, r: &Resolve) -> Result<Self> {
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




/*
/// `/Type Page`
qtyped!(Page {
    parent: PageTree,
    resources: Option<Resources>,
});
/// `/Type Pages`
qtyped!(PageTree {
    parent: Option<PageTree>,
    kids: Vec<ObjectId>,
    count: i32,
    resources: Option<Resources>,
});
/// `/Type Resources` - resource dictionary.
qtyped!(Resources {
    ext_g_state: Option<ExtGState>,
    color_space: Dictionary,
    // TODO:
    // Pattern
    // Shading
    // XObject
    // Font
    // ProcSet
    // Properties

});

/// `/Type ExtGState` - graphics state parameter dictionary.
qtyped!(ExtGState {
    line_width: Option<String>,
    line_cap_style: Option<i32>,
    line_join_style: Option<i32>,
    // TODO ETC
});

/// `/Type Catalog`
pub struct Catalog {
    pub version: Option<String>,
    /// `/Pages`
    pub page_tree: PageTree,
    // TODO PageLabels
    pub names: Option<Dictionary>,
    // TODO rest
}
 */
