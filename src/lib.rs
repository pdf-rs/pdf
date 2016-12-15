// #![feature(plugin)]
// #![plugin(clippy)]
// //

#[macro_use (o, slog_log, slog_trace, slog_debug, slog_info, slog_warn, slog_error)]
extern crate slog;
extern crate slog_json;
#[macro_use]
extern crate slog_scope;
extern crate slog_stream;
extern crate slog_term;
extern crate isatty;

pub mod file_reader;
pub mod repr;
pub mod error;


// Thoughts...
// Method 1
// - We load string into memory
// - We need runtime repr of xref table
//         and store /Root
// - When we need an object, just look in xref table and read it straight from the string
//
//
// Method 2
// But what about representing the whole PDF as a kind of struct?
//  - It should be able to write back the exact file it reads in.
//  - This means it will just be a tree of (Indirect) Objects, each Object containing any amount of items.



// Pros/cons
//
// Method 1
//  - PDF is created for this kind of access:
//      - xref table tells where things are, so we don't need to parse things
//        before they are needed
//      - modifying a PDF file is done by only writing to the very end of the file
// Method 2
//  - Allows construction easily
//  - Will take less RAM

// Plan:
// First don't care about storing structures. Just use Lexer to parse things whenever needed.


#[cfg(test)]
mod tests {
    use file_reader::PdfReader;
    use repr::*;

    use std;
    use std::io;
    use std::fs::File;
    use std::io::{Seek, Read};
    use std::vec::Vec;
    use file_reader::lexer::Lexer;
    use slog;
    use slog::{DrainExt, Level};
    use {slog_term, slog_stream, isatty, slog_json, slog_scope};

    const EXAMPLE_PATH: &'static str = "example.pdf";

    //#[test]
    fn sequential_read() {
        let buf = read_file(EXAMPLE_PATH);
        println!("\nSEQUENTIAL READ\n");

        let mut lexer = Lexer::new(&buf);
        let mut substr = None;
        loop {
            let lexeme = lexer.next();
            match lexeme {
                None => break,
                Some(lexeme) => {
                    if lexeme.equals(b"%") {
                        lexer.seek_newline();
                    } else if lexeme.equals(b"stream") {
                        substr = Some(lexer.seek_substr(b"endstream").unwrap());
                    } else {
                        println!("{}", lexeme.as_str());
                    }
                }
            }
        }
        match substr {
            None => println!("No substr.."),
            Some(substr) => println!("Stream: {}", substr.as_str()),
        }
    }

    #[test]
    fn structured_read() {
        setup_logger();

        let reader = PdfReader::new(EXAMPLE_PATH);
        let val = reader.trailer.dictionary_get(Name(String::from("Root")));
        match val {
            Some(obj) => {
                info!("Trailer"; "trailer" => obj.to_string());
            },
            None => panic!("val = None"),
        }
    }


    fn read_file(path: &str) -> Vec<u8> {
        let mut file  = File::open(path).unwrap();
        let length = file.seek(io::SeekFrom::End(0)).unwrap();
        file.seek(io::SeekFrom::Start(0)).unwrap();
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(length as usize, 0);
        let _ = file.read(&mut buf); // Read entire file into memory

        buf
    }

    fn setup_logger() {
        let logger = if isatty::stderr_isatty() {
            let drain = slog_term::streamer()
                .async()
                .stderr()
                .full()
                .use_utc_timestamp()
                .build();
            let d = slog::level_filter(Level::Trace, drain);
            slog::Logger::root(d.fuse(), o![])
        } else {
            slog::Logger::root(slog_stream::stream(std::io::stderr(), slog_json::default()).fuse(),
                               o![])
        };
        slog_scope::set_global_logger(logger);
    }
}
