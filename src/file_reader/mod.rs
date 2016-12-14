pub mod lexer;

use repr::*;

use self::lexer::Lexer;
use std::vec::Vec;
use std::io::SeekFrom;
use std::io::Seek;
use std::io::Read;
use std::fs::File;

// TODO in the whole file: proper error handling with Result.

// PLAN
// I don't know the best way to read from file... keep entire file in memory as Vec<u8> or nested
// structure?
//
// So I think it is best to first store nothing but important findings such as xref position...

pub struct PdfReader {
    pub trailer: Object,
    startxref: usize,
    xref_table: XrefTable,
    buf: Vec<u8>,
}


impl PdfReader {
    pub fn new(path: &str) -> PdfReader {
        let buf = read_file(path);
        let mut pdf_reader = PdfReader {
            trailer: Object::Null,
            xref_table: XrefTable::new(0),
            startxref: 0,
            buf: buf,
        };
        pdf_reader.read_trailer();

        let start = pdf_reader.startxref;
        pdf_reader.xref_table = pdf_reader.read_xref(start);

        pdf_reader
    }

    pub fn read_xref(&mut self, start: usize) -> XrefTable {
        let mut lexer = Lexer::new(&self.buf);
        // Read xref
        lexer.seek(SeekFrom::Start(start as u64));
        let word = lexer.next().unwrap();
        assert!(word.as_str() == "xref");

        let start_id = lexer.next().unwrap().to::<usize>().unwrap();
        let num_ids = lexer.next().unwrap().to::<usize>().unwrap();

        let mut table = XrefTable::new(start_id);

        for _ in start_id..(start_id+num_ids) {
            let w1 = lexer.next().unwrap();
            let w2 = lexer.next().unwrap();
            let w3 = lexer.next().unwrap();
            if w3.equals(b"f") {
                table.add_free_entry(w1.to::<usize>().unwrap(), w2.to::<usize>().unwrap());
            } else if w3.equals(b"n") {
                table.add_inuse_entry(w1.to::<usize>().unwrap(), w2.to::<usize>().unwrap());
            } else { // ??
            }
        }
        table
    }

    /// Needs to be called before any other functions on the PdfReader
    fn read_trailer(&mut self) {
        let mut lexer = Lexer::new(&self.buf);

        // Find startxref
        lexer.seek(SeekFrom::End(0));
        let _ = lexer.seek_substr_back(b"startxref").expect("Could not find startxref!");
        self.startxref = lexer.next().expect("no startxref entry").to::<usize>().unwrap();

        // Find trailer start
        let _ = lexer.seek_substr_back(b"trailer");
        self.trailer = self.read_object(&mut lexer);
    }

    /// Reads object starting at where the `Lexer` is currently at.
    fn read_object(&self, lexer: &mut Lexer) -> Object {
        let first_lexeme = lexer.next().unwrap();

        //(TODO is it possible to use match instead of all these else-ifs?
        if first_lexeme.equals(b"<<") {
            let mut dictionary = Vec::new();
            loop {
                // Expect a Name (and Object) or the '>>' delimiter
                let delimiter = lexer.next().unwrap();
                if delimiter.equals(b"/") {
                    let name = Name(String::from(lexer.next().unwrap().as_str()));
                    let obj = self.read_object(lexer);
                    dictionary.push( (name, obj) );
                } else if delimiter.equals(b">>") {
                    return Object::Dictionary(dictionary);
                } else {
                    panic!("Error reading dictionary. Found {}", delimiter.as_str());
                }
            }
        } else if first_lexeme.is_integer() {
            // Test to see if this is a reference rather than integer.
            // First backup position
            let pos_bk = lexer.get_pos();
            
            let second_lexeme = lexer.next().unwrap();
            if second_lexeme.is_integer() {
                let third_lexeme = lexer.next().unwrap();
                if third_lexeme.equals(b"R") {
                    // It is indeed a reference to an indirect object
                    return Object::Reference {
                        obj_nr: first_lexeme.to::<i32>().unwrap(),
                        gen_nr: second_lexeme.to::<i32>().unwrap(),

                    };
                } else {
                    panic!("Inclompete reference {} {} {}.",
                           first_lexeme.to::<usize>().unwrap(),
                           second_lexeme.to::<usize>().unwrap(),
                           third_lexeme.as_str());
                }
            } else {
                // It is but a number
                lexer.seek(SeekFrom::Start(pos_bk as u64)); // (roll back the lexer first)
                return Object::Integer(first_lexeme.to::<i32>().unwrap());
            }
        }
        Object::Null
    }

    fn read_indirect_object(&mut self, start_pos: usize) -> IndirectObject {
        let mut lexer = Lexer::new(&self.buf);
        lexer.seek(SeekFrom::Start(start_pos as u64));
        let obj_nr = lexer.next().unwrap().to::<i32>().unwrap();
        let gen_nr = lexer.next().unwrap().to::<i32>().unwrap();
        let obj_literal = lexer.next().unwrap();
        assert!(obj_literal.equals(b"obj"));

        let obj = self.read_object(&mut lexer);

        let endobj_literal = lexer.next().unwrap();
        assert!(endobj_literal.equals(b"endobj"));

        IndirectObject {
            obj_nr: obj_nr,
            gen_nr: gen_nr,
            object: obj,
        }
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
