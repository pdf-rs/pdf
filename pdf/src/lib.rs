//! Read, alter and write PDF files.
//!
//! This is a low level library: it gives you the objects a PDF file is made of
//! and leaves the decisions to you. It does not render pages and does not
//! extract text — for those see
//! [`pdf_render`](https://github.com/pdf-rs/pdf_render) and
//! [`pdf_text`](https://github.com/pdf-rs/pdf_text).
//!
//! The API follows the document format closely, so *PDF 32000-1:2008* is the
//! real reference. Where an item corresponds to a construct in the
//! specification, its documentation names the clause — `PDF 32000-1:2008
//! §7.6.5` — which is the shortest path from a keyword in a broken file
//! (`/StrF`) to the code that handles it. The [module map](#specification-map)
//! below does the same for whole modules.
//!
//! # Opening a file
//!
//! ```no_run
//! use pdf::file::FileOptions;
//!
//! # fn main() -> Result<(), pdf::PdfError> {
//! let file = FileOptions::cached().open("files/example.pdf")?;
//! let resolver = file.resolver();
//!
//! for page in file.pages() {
//!     let page = page?;
//!     if let Some(content) = &page.contents {
//!         for op in content.operations(&resolver)? {
//!             // Op::ShowText, Op::SetMatrix, ... — draw them, or write them back
//!             let _ = op;
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Everything that is read out of the file is *lazy*: a [`Ref`] becomes a value
//! when you call [`Resolve::get`], and streams are decoded when you ask for
//! their data. [`File`] keeps the bytes, a resolver borrows them.
//!
//! [`Ref`]: object::Ref
//! [`Resolve::get`]: object::Resolve::get
//! [`File`]: file::File
//!
//! Encrypted files take a password, which is tried as both the user and the
//! owner password (`PDF 32000-1:2008 §7.6.4`):
//!
//! ```no_run
//! # use pdf::file::FileOptions;
//! # fn main() -> Result<(), pdf::PdfError> {
//! let file = FileOptions::cached().password(b"userpassword").open("files/enc.pdf")?;
//! # let _ = file.num_pages();
//! # Ok(())
//! # }
//! ```
//!
//! # Layers
//!
//! From the bytes up; each layer only needs the one below it.
//!

//!
//! # Tolerance
//!
//! Real files break the specification all the time, so what counts as an error
//! is a choice: [`ParseOptions::strict`] for input you trust, and
//! [`ParseOptions::tolerant`] for input you do not — the latter also relaxes the
//! object size limit that guards against decompression bombs. The default of
//! [`FileOptions::uncached`] is strict, which is the opposite of what most
//! callers of a PDF library want; pick deliberately.
//!
//! [`ParseOptions::strict`]: object::ParseOptions::strict
//! [`ParseOptions::tolerant`]: object::ParseOptions::tolerant
//!
//! # What is not here
//!
//! Rendering, text extraction, and repair. Spec constructs that are parsed but
//! not acted on, and known deviations, are recorded per module in the `<file>.rs.md`
//! notes next to the source (for example `pdf/src/crypt.rs.md`); treat those as
//! the list of things this library does not do, before trusting a feature.
//! Writing is the least mature part: `build.rs` and incremental updates work,
//! rewriting an arbitrary document does not.
//!
//! [`Primitive`]: primitive::Primitive
//! [`FileOptions::uncached`]: file::FileOptions::uncached

#![allow(non_camel_case_types)]  /* TODO temporary becaues of pdf_derive */
#![allow(unused_doc_comments)] // /* TODO temporary because of err.rs */
#![allow(clippy::len_zero, clippy::should_implement_trait, clippy::manual_map, clippy::from_over_into)]
#![deny(ambiguous_glob_imports)]

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
pub mod build;

// mod content;
pub mod enc;
pub mod crypt;

// pub use content::*;
pub use crate::error::PdfError;
