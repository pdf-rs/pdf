use doc::Document;
use file;
use err::*;
use std::fmt;
// use std::fmt::{Formatter, Debug};

// Want to wrap file::Primitive together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.


pub trait Object {
    fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()>;
}

/// Wraps `file::Stream`.
#[derive(Clone)]
pub struct Stream {
    pub dict: Dictionary,
    
    // decoded data
    pub data: Vec<u8>
}


impl<'a> fmt::Debug for Stream<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Dict: {:?}, Content: {:?}", self.dict, self.data)
    }
}
