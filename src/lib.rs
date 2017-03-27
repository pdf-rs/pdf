#![feature(attr_literals)] 
//#![feature(collections_range)]
//#![feature(slice_get_slice)]

#[macro_use]
extern crate pdf_derive;
#[macro_use]
extern crate error_chain;
extern crate num_traits;
extern crate inflate;
extern crate ansi_term;
extern crate byteorder;
extern crate itertools;
extern crate ordermap;
extern crate memmap;
extern crate encoding;

#[macro_use]
mod macros;
pub mod parser;
pub mod object;
pub mod types;
pub mod xref;
pub mod primitive;
pub mod stream;
pub mod file;
pub mod backend;

mod err;
// mod content;

// pub use content::*;
pub use err::*;

// hack to use ::pdf::object::Object in the derive
mod pdf {
    pub use super::*;
}

/// Prints the error if it is an Error
pub fn print_err<T>(err: Error) -> T {
    println!("\n === \nError: {}", err);
    for e in err.iter().skip(1) {
        println!("  caused by: {}", e);
    }
    println!(" === \n");

    if let Some(backtrace) = err.backtrace() {
        println!("backtrace: {:?}", backtrace);
    }

    println!(" === \n");
    panic!("Exiting");
}

#[cfg(test)]
mod tests {
}
