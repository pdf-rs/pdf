//! Basic functionality for parsing a PDF file.
mod lexer;
mod reader;
mod writer;

use primitive::Primitive;

pub use self::reader::*;
pub use self::writer::*;

pub fn parse(data: &[u8]) -> Primitive {
    unimplemented!()
}
