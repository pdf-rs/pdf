#![allow(non_camel_case_types)]  /* TODO temporary becaues of pdf_derive */
#![allow(unused_doc_comments)] // /* TODO temporary because of err.rs */
#![feature(custom_attribute)]
#![feature(core_intrinsics)]

#[macro_use] extern crate pdf_derive;
#[macro_use] extern crate snafu;
#[macro_use] extern crate log;

#[macro_use]
pub mod error;
pub mod object;
pub mod xref;
pub mod primitive;
pub mod file;
pub mod backend;
pub mod content;
pub mod parser;
pub mod font;
pub mod any;
pub mod encoding;

// mod content;
mod enc;
pub mod crypt;

// pub use content::*;
pub use crate::error::PdfError;
