#![feature(attr_literals)]
#![recursion_limit="128"]
//#![feature(collections_range)]
//#![feature(slice_get_slice)]
#![allow(non_camel_case_types)]  /* TODO temporary becaues of pdf_derive */
    #![allow(unused_doc_comments)] // /* TODO temporary because of err.rs */
#![feature(use_extern_macros)] // because of error-chain experimenting
#[macro_use]
extern crate pdf_derive;
#[macro_use]
extern crate error_chain;
extern crate num_traits;
extern crate inflate;
extern crate itertools;
extern crate memmap;
extern crate tuple;
extern crate chrono;

//#[macro_use]
//mod macros;
pub mod parser;
pub mod object;
pub mod xref;
pub mod primitive;
pub mod file;
pub mod backend;
pub mod content;

mod err;
// mod content;
mod enc;

// pub use content::*;
pub use err::*;

// hack to use ::pdf::object::Object in the derive
mod pdf {
    pub use super::*;
}

/// Prints the error if it is an Error
pub fn print_err<T>(err: Error) -> T {
    use std;

    // Get path of project... kinda silly way.
    let mut proj_path = std::env::current_exe().unwrap();
    proj_path.pop(); proj_path.pop(); proj_path.pop(); proj_path.pop();
    proj_path.push("src");

    println!("PATH: {:?}", proj_path);
    println!("\n === \nError: {}", err);
    for e in err.iter().skip(1) {
        println!("  caused by: {}", e);
    }
    println!(" === \n");

    if let Some(backtrace) = err.backtrace() {
        for frame in backtrace.frames() {
            for symbol in frame.symbols() {
                if let Some(path) = symbol.filename() {
                    if let Some(lineno) = symbol.lineno() {
                        if proj_path < path {
                            println!("\tat {:?}:{}", path, lineno);
                        }
                    }
                }
            }
        }
    }

    println!(" === \nExiting...");
    panic!("");
}
