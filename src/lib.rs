// #![feature(plugin)]
// #![plugin(clippy)]

#[macro_use]
extern crate error_chain;
extern crate num_traits;
extern crate inflate;
extern crate ansi_term;

pub mod reader;
pub mod object;
pub mod xref;
pub mod err;
pub mod content;

// TODO Plan

// In progress now: Remove use of Slog and just use println instead.
// * Test more extensively
// * Write back to file - that means keeping track of what has changed


// TODO Future:
// - Choose to read everything into a high-level intermediate representation for faster access &
// less memory.
// - Choose to read directly from file.

// Later there should be an option to read directly from file

#[cfg(test)]
mod tests {
    use reader;
    use reader::PdfReader;
    use reader::lexer::Lexer;
    use reader::lexer::StringLexer;
    use object::*;
    use xref::*;
    use err::*;

    use std;
    use ansi_term::Style;

    //#[test]
    fn sequential_read() {
        let buf = reader::read_file("edited_example.pdf").chain_err(|| "Cannot read file.").unwrap_or_else(|e| print_err(e));
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

    #[test]
    fn read_xref() {
        let reader = PdfReader::new("la.pdf").chain_err(|| "Error creating PdfReader.").unwrap_or_else(|e| print_err(e));
        println!("\n          {}\n\n{:?}\n\n",
                 Style::new().bold().underline().paint("Xref Table"),
                 reader.get_xref_table()
                 );
    }

    #[test]
    fn read_pages() {
        let reader = PdfReader::new("la.pdf").chain_err(|| "Error creating PdfReader.").unwrap_or_else(|e| print_err(e));

        let n = reader.get_num_pages();
        for i in 0..n {
            println!("Reading page {}", i);
            let page = reader.find_page(i).chain_err(|| format!("Get page {}", i)).unwrap_or_else(|e| print_err(e));
            for (& ref name, & ref object) in &page.0 {
                let object = reader.dereference(object).chain_err(|| "Dereferencing an object...").unwrap_or_else(|e| print_err(e));
                match object {
                    Object::Array (ref arr) => {
                        for (i, e) in arr.iter().enumerate() {
                            let e = reader.dereference(e).chain_err(|| "Deref element").unwrap_or_else(|e| print_err(e));
                            println!(" [{}] = {}\n", i, e);
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
    fn read_string() {
        let s: &[u8] = "(\\2670\\331\\346\\nZ\\356\\215n\\273\\264\\350d \\013t\\2670\\331\\346\\nZ\\356\\215n\\273\\264\\350d\n \\013t\\\n)".as_bytes();
        let mut lexer = StringLexer::new(s);
        for c in lexer.iter() {
            let c = c.unwrap_or_else(|e| print_err(e));
            print!("{}, ", c);
        }
    }

    /// Prints the error if it is an Error
    fn print_err<T>(err: Error) -> T {
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


}
