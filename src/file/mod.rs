//! Basic functionality for parsing a PDF file.
pub mod lexer;
mod reader;
mod writer;
mod parse_object;
mod parse_xref;
mod object;
mod xref;

pub use self::object::*;
pub use self::xref::*;
pub use self::reader::*;
pub use self::writer::*;

use err::*;

use self::lexer::Lexer;
use std::vec::Vec;
use std::io::SeekFrom;
use std::io::Seek;
use std::io::Read;
use std::fs::File;
use std::iter::Iterator;

