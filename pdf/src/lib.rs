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
//! | layer | modules | what it holds |
//! |---|---|---|
//! | values | [`primitive`] | [`Primitive`], the nine kinds of object a PDF file is made of |
//! | types | [`object`], [`font`], [`encoding`] | typed views of those values — `Catalog`, `Page`, `Font`, `XObject`, … — and the traits that convert both ways |
//! | syntax | [`parser`], [`content`] | the lexer, the object parser, and content streams as a list of operators |
//! | file | [`xref`], [`file`], [`backend`] | cross-reference sections, the byte store, caches, `save_to` and `rebuild` |
//! | data | [`enc`], [`crypt`] | stream filters and predictors, RC4/AES decryption |
//! | writing | [`build`] | building a document up from nothing, and importing objects into it |
//!
//! # Specification map
//!
//! Which module answers to which part of *PDF 32000-1:2008*. Read it in the
//! other direction too: given a failing page, this is where its bytes are
//! interpreted.
//!
//! | module | clause | covers |
//! |---|---|---|
//! | [`primitive`] | §7.3 (7.3.2–7.3.9), §7.9.4, §7.9.5 | the untyped values: boolean, number, string, name, array, dictionary, null; dates, rectangles |
//! | [`object`] | §7.3.8, §7.3.10 | streams and indirect objects; the traits that turn values into types |
//! | `object/types.rs`, `object/types/page.rs` | §7.7.2, §7.7.3 (7.7.3.3, 7.7.3.4), §7.7.4 | catalog, page tree and object, inherited page attributes, name dictionary |
//! | `object/types/nametree.rs`, `numbertree.rs` | §7.9.6, §7.9.7 | name and number trees |
//! | `object/function.rs` | §7.10 | type 0–4 functions, sampled and PostScript ones |
//! | `object/color.rs` | §8.6 | colour spaces, device-independent and special |
//! | `object/types/xobject.rs` | §8.8, §8.9, §8.10 | image and form XObjects |
//! | `object/types/graphicsstate.rs`, `pattern.rs` | §8.4, §8.7 | graphics state parameters, patterns |
//! | `object/types/dest.rs`, `outline.rs` | §12.3.2.2, §12.3.2.3, §12.3.3 | explicit and named destinations, the document outline |
//! | `object/types/form.rs` | §12.5, §12.7 | annotations, interactive forms |
//! | `object/types/structtree.rs` | §14.7 | logical structure (tagged PDF) |
//! | [`parser`] | §7.2, §7.3.4, §7.5.4 | lexical conventions, literal and hex strings, the xref table |
//! | [`xref`] | §7.5.4, §7.5.6, §7.5.8 | xref table and stream entries, incremental updates |
//! | `object/stream.rs` | §7.5.7 | object streams |
//! | [`file`], [`backend`] | §7.5.2, §7.5.3, §7.5.5 | header, body, trailer; the byte store behind all of it |
//! | [`enc`] | §7.4 (7.4.2–7.4.10) | the filters, and the predictors of Flate and LZW |
//! | [`crypt`] | §7.6 (7.6.2, 7.6.3) | encryption dictionaries, the standard security handler |
//! | [`content`] | §7.8.2, Annex A, §8.3 | content streams as operators, the `Op` enum, the text matrix |
//! | [`font`], [`encoding`] | §9 (9.6, 9.7, 9.8) | simple and composite fonts, font descriptors, encodings, `ToUnicode` |
//! | [`build`] | §7.5.4, §7.5.6 | writing a file from scratch, and incremental updates |
//!
//! [`mod@error`] and [`any`] have no clause: one is this library's error type, the
//! other the type-erased objects the caches hold.
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
// The documentation is part of the product, so a dead link in it is a bug in
// the same way a compile error is. `missing_docs` is not denied (yet) — that
// is a job for when the module-by-module pass is done; see CODE_INDEX.md.
#![deny(
    rustdoc::broken_intra_doc_links,
    rustdoc::private_intra_doc_links,
    rustdoc::bare_urls,
    rustdoc::invalid_html_tags,
    rustdoc::unescaped_backticks
)]

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
