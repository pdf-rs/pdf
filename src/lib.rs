#![feature(attr_literals)] 

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

#[macro_use]
mod macros;
pub mod file;
pub mod object;
pub mod types;
pub mod xref;
pub mod primitive;
pub mod stream;

mod err;
// mod content;
pub mod document;

// pub use content::*;
pub use err::*;

// hack to use ::pdf::object::Object in the derive
mod pdf {
    pub use super::*;
}
// TODO
// - impl Into<Object>
// - Consider whether we should enumerate all operations and graphics/text state parameters

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
    use ::print_err;
    use file;
    use file::Reader;
    use file::lexer::Lexer;
    use file::lexer::StringLexer;
    use file::*;
    use err::*;
    use Content;

    use std::str;
    use ansi_term::Style;

    //#[test]
    #[allow(dead_code)]
    fn sequential_read() {
        let buf = file::read_file("edited_example.pdf").chain_err(|| "Cannot read file.").unwrap_or_else(|e| print_err(e));
        let mut lexer = Lexer::new(&buf);
        loop {
            let pos = lexer.get_pos();
            let next = match lexer.next() {
                Ok(next) => next,
                Err(Error (ErrorKind::EOF, _)) => break,
                Err(e) => print_err(e),
            };
            println!("{}\t{}", pos, next.as_string());
        }
        /*
        loop {
            let next = match lexer.back() {
                Ok(next) => next,
                Err(Error (ErrorKind::EOF, _)) => break,
                Err(e) => print_err(e),
            };
            println!("word: {}", next.as_string());
        }
        */
    }

    // #[test]
    #[allow(dead_code)]
    fn read_xref() {

        let reader = Reader::from_path("la.pdf").chain_err(|| "Error creating Reader.").unwrap_or_else(|e| print_err(e));
        println!("\n          {}\n\n{:?}\n\n",
                 Style::new().bold().underline().paint("Xref Table"),
                 reader.get_xref_table()
                 );
    }

    //#[test]
    #[allow(dead_code)]
    fn read_pages() {
        let reader = Reader::from_path("la.pdf").chain_err(|| "Error creating Reader.").unwrap_or_else(|e| print_err(e));

        let n = reader.get_num_pages();
        for i in 0..n {
            println!("Reading page {}", i);
            let page = reader.find_page(i).chain_err(|| format!("Get page {}", i)).unwrap_or_else(|e| print_err(e));
            for (& ref name, & ref object) in &page {
                let object = reader.dereference(object).chain_err(|| "Dereferencing an object...").unwrap_or_else(|e| print_err(e));
                match object {
                    Primitive::Array (ref arr) => {
                        println!("/{} =\n\n", name);
                        for (i, e) in arr.iter().enumerate() {
                            let e = reader.dereference(e).chain_err(|| "Deref element").unwrap_or_else(|e| print_err(e));
                            println!(" [{}] = {}\n", i, e);
                            if name == "Contents" {
                                // Decode the contents into operators & operands
                                let stream = e.as_stream().unwrap_or_else(|e| print_err(e));
                                let contents = Content::parse_from(&stream.content).unwrap_or_else(|e| print_err(e));
                                println!(" Contents: {}", contents);
                            }
                        }
                    }
                    _ => {
                        println!("/{} =\n\n{}\n\n", name, object);
                    }
                }
            }
        }
    }


    // #[test]
    #[allow(dead_code)]
    fn read_string() {
        let s: &[u8] = "(\\2670\\331\\346\\nZ\\356\\215n\\273\\264\\350d \\013t\\2670\\331\\346\\nZ\\356\\215n\\273\\264\\350d\n \\013t\\\n)".as_bytes();
        let mut lexer = StringLexer::new(s);
        for c in lexer.iter() {
            let c = c.unwrap_or_else(|e| print_err(e));
            print!("{}, ", c);
        }
    }

    // #[test]
    #[allow(dead_code)]
    fn read_string2() {
        let buf = b"[(Problem)-375(Set)-375(2,)-375(P)31(art)-374(1)]";
        println!("Test: {}", str::from_utf8(buf).unwrap());
        let mut lexer = Lexer::new(buf);

        let reader = Reader::new(buf.to_vec()).unwrap_or_else(|e| print_err(e));
        let obj = reader.parse_object(&mut lexer).unwrap_or_else(|e| print_err(e));
        println!("Object: {}", obj);
    }


}
