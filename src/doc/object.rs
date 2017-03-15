use doc::Document;
use file::{Primitive, Reader};
use err::*;
use std::{io, fmt};

// use std::fmt::{Formatter, Debug};

// Want to wrap file::Primitive together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.

pub trait Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>;
}

pub trait PrimitiveConv {
    fn from_primitive(p: &Primitive, reader: &Reader) -> Self;
}
