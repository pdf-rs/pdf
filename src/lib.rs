pub mod file_reader;
pub mod repr;
pub mod error;

/* #[macro_use]
extern crate error_chain; */



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


// Remember:
// Usually, there is an expected type of an object that is referenced.
//  - except for example Stream /Filter, which can be a Name or a Dictionary.

// Method 1
//  - PDF is created for this kind of access:
//      - xref table tells where things are, so we don't need to parse things
//        before they are needed
//      - modifying a PDF file is done by only writing to the very end of the file
// Method 2
//  - Allows construction easily
//  - Will take less RAM
//
// Is there a way to make this library to support both ways? Start with method 1, later extend it
// to method 2?
// Let's be concerned only about reading & understanding a PDF file.
// private methods `get_object(obj_nr, gen_nr)`
//  - method 1 looks in xref table, then parses the file
//  - method 2 just gives the object
//  - Both methods will need to return something..


#[cfg(test)]
mod tests {
    use repr::PDF;

    use std::io;
    use std::fs::File;
    use std::io::{Write, BufReader, Seek, Read};
    use std::vec::Vec;
    use file_reader::lexer::Lexer;
    use std::io::SeekFrom;

    const example_path: &'static str = "example.pdf";

    #[test]
    fn sequential_read() {
        let buf = read_file(example_path);
        println!("\nSEQUENTIAL READ\n");

        let lexer = Lexer::new(buf);
        let mut iter = lexer.iter();
        let mut substr = None;
        loop {
            let lexeme = iter.next();
            match lexeme {
                None => break,
                Some(lexeme) => {
                    if lexeme.equals(b"%") {
                        iter.seek_newline();
                    } else if lexeme.equals(b"stream") {
                        substr = Some(iter.seek_substr(b"endstream").unwrap());
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
        let buf = read_file(example_path);
        let lexer = Lexer::new(buf);
        let mut iter = lexer.iter();
        iter.seek(SeekFrom::End(0));
    }


    fn read_file(path: &str) -> Vec<u8> {
        let path =  "example.pdf";
        let mut file  = File::open(path).unwrap();
        let length = file.seek(io::SeekFrom::End(0)).unwrap();
        file.seek(io::SeekFrom::Start(0)).unwrap();
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(length as usize, 0);
        file.read(&mut buf); // Read entire file into memory

        buf
    }
}
