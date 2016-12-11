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

pub struct PdfReader {
    startxref: usize,
    buf: Vec<u8>,
}


impl PdfReader {
    pub fn new(path: &str) -> PdfReader {
        let buf = read_file(path);
        let mut result = PdfReader {
            startxref: 0,
            buf: buf,
        };
        result.read_trailer();
        result
    }

    pub fn read_xref(&mut self) {
        let mut lexer = Lexer::new(&self.buf);
        // Read xref
        lexer.seek(SeekFrom::Start(self.startxref as u64));
        let word = lexer.next().unwrap();
        assert!(word.as_str() == "xref");

        let start_id = lexer.next().unwrap().to::<usize>();
        let num_ids = lexer.next().unwrap().to::<usize>();

        for id in start_id..(start_id+num_ids) {
            // TODO 
        }
    }

    fn read_trailer(&mut self) {
        let mut lexer = Lexer::new(&self.buf);

        // Find startxref
        lexer.seek(SeekFrom::End(0));
        let _ = lexer.seek_substr_back(b"startxref").expect("Could not find startxref!");
        self.startxref = lexer.next().expect("no startxref entry").to::<usize>();
    }
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
