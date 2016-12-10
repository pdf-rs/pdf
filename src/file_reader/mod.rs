pub mod lexer;

use self::lexer::Lexer;
use std::vec::Vec;
use std::io::SeekFrom;
use std::io::Seek;
use std::io::Read;
use std::fs::File;

// PLAN
// I don't know the best way to read from file... keep entire file in memory as Vec<u8> or nested
// structure?
//
// So I think it is best to first store nothing but important findings such as xref position...

pub struct PdfReader<'a> {
    xref_pos: usize,
    lexer: Lexer<'a>,
}


impl<'a> PdfReader<'a> {
    /*
    pub fn new(path: &str) -> PdfReader {
        PdfReader {
            xref_pos: 0,
            lexer: Lexer::new(read_file(path)),
        }
    }

    pub fn read(&mut self) {

    }

    fn read_trailer(&mut self) {
        let mut lexer = Lexer::new(&self.buf);

        // Find startxref
        lexer.seek(SeekFrom::End(0));
        let substr = lexer.seek_substr_back(b"startxref").expect("Could not find startxref!");
        let startxref = lexer.next().expect("no startxref entry").to::<usize>();
    }
    */
}

fn read_file(path: &str) -> Vec<u8> {
    let path =  "example.pdf";
    let mut file  = File::open(path).unwrap();
    let length = file.seek(SeekFrom::End(0)).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut buf: Vec<u8> = Vec::new();
    buf.resize(length as usize, 0);
    file.read(&mut buf); // Read entire file into memory

    buf
}
