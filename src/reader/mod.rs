mod lexer;

use repr::*;

use error::{Result, Error};

use self::lexer::Lexer;
use std::vec::Vec;
use std::io::SeekFrom;
use std::io::Seek;
use std::io::Read;
use std::fs::File;

// TODO in the whole file: proper error handling with Result.

pub struct PdfReader {
    pub trailer: Object,
    startxref: usize,
    xref_table: XrefTable,
    buf: Vec<u8>,
}


impl PdfReader {
    // TODO what if it's not created by new()? Do we need to invalidate it?
    pub fn new(path: &str) -> Result<PdfReader> {
        let buf = read_file(path);
        let mut pdf_reader = PdfReader {
            trailer: Object::Null,
            xref_table: XrefTable::new(0),
            startxref: 0,
            buf: buf?,
        };
        pdf_reader.read_trailer()?;

        let start = pdf_reader.startxref;
        pdf_reader.xref_table = pdf_reader.read_xref(start)?;

        Ok(pdf_reader)
    }

    pub fn read_xref(&mut self, start: usize) -> Result<XrefTable> {
        let mut lexer = Lexer::new(&self.buf);

        // Read xref
        lexer.seek(SeekFrom::Start(start as u64));
        let word = lexer.next()?;
        if word.as_str() != "xref" {
            return Err(Error::InvalidXref{pos: lexer.get_pos()});
        }

        let start_id = lexer.next_as::<u32>()?;
        let num_ids = lexer.next_as::<u32>()?;

        let mut table = XrefTable::new(start_id);

        for _ in 0..num_ids {
            let w1 = lexer.next()?;
            let w2 = lexer.next()?;
            let w3 = lexer.next()?;
            if w3.equals(b"f") {
                table.add_free_entry(w1.to::<u32>()?, w2.to::<u16>()?);
            } else if w3.equals(b"n") {
                table.add_inuse_entry(w1.to::<usize>()?, w2.to::<u16>()?);
            } else {
                // ??
            }
        }
        Ok(table)
    }

    /// Needs to be called before any other functions on the PdfReader
    fn read_trailer(&mut self) -> Result<()> {
        let mut lexer = Lexer::new(&self.buf);

        // Find startxref
        lexer.seek(SeekFrom::End(0));
        let _ = lexer.seek_substr_back(b"startxref")?;
        self.startxref = lexer.next_as::<usize>()?;

        // Find trailer start
        let _ = lexer.seek_substr_back(b"trailer")?;
        self.trailer = self.read_object(&mut lexer)?;
        Ok(())
    }

    /// Reads object starting at where the `Lexer` is currently at.
    fn read_object(&self, lexer: &mut Lexer) -> Result<Object> {
        let first_lexeme = lexer.next()?;

        if first_lexeme.equals(b"<<") {
            let mut dictionary = Vec::new();
            loop {
                // Expect a Name (and Object) or the '>>' delimiter
                let delimiter = lexer.next()?;
                if delimiter.equals(b"/") {
                    let name = Name(String::from(lexer.next()?.as_str()));
                    let obj = self.read_object(lexer)?;
                    dictionary.push( (name, obj) );
                } else if delimiter.equals(b">>") {
                    break;
                } else {
                    return Err(Error::UnexpectedToken{ pos: lexer.get_pos(), token: delimiter.as_string(), expected: "/ or >>"});
                }
            }
            // It might just be the dictionary in from of a stream.
            let dict = Object::Dictionary(dictionary.clone());
            if lexer.next()?.equals(b"stream") {
                // (TODO but does it really have to have /Length?)
                // Get length
                let length_obj = dict.dictionary_get(String::from("Length"))
                    .expect("No length of stream specified."); // TODO error handling

                let length = // TODO shorten
                    if let &Object::Reference{ obj_nr, gen_nr:_ } = length_obj {
                        if let Object::Integer(length) = self.read_indirect_object(obj_nr)?.object {
                            length
                        } else {
                            // Expected integer
                            return Err(Error::UnexpectedType{ pos: lexer.get_pos()});
                        }
                    } else {
                        // Expected reference.
                        return Err(Error::UnexpectedType{ pos: lexer.get_pos()})
                    };
                // Read the stream
                let content = lexer.seek(SeekFrom::Current(length as i64));
                // Finish
                let endstream_literal = lexer.next()?;
                if !endstream_literal.equals(b"endstream") {
                    return Err(Error::UnexpectedToken {pos: lexer.get_pos(), token: endstream_literal.as_string(), expected: "endstream"} );
                }

                Ok(Object::Stream {
                    filters: Vec::new(),
                    dictionary: dictionary,
                    content: String::from(content.as_str()),
                })
            } else {
                Ok(dict)
            }
        } else if first_lexeme.is_integer() {
            // Test to see if this is a reference rather than integer.
            // First backup position
            let pos_bk = lexer.get_pos();
            
            let second_lexeme = lexer.next()?;
            if second_lexeme.is_integer() {
                let third_lexeme = lexer.next()?;
                if third_lexeme.equals(b"R") {
                    // It is indeed a reference to an indirect object
                    Ok(Object::Reference {
                        obj_nr: first_lexeme.to::<i32>()?,
                        gen_nr: second_lexeme.to::<i32>()?,
                    })
                } else {
                    // The reference is incomplete
                    return Err(Error::UnexpectedToken {pos: lexer.get_pos(), token: third_lexeme.as_string(), expected: "R"});
                }
            } else {
                // It is but a number
                lexer.seek(SeekFrom::Start(pos_bk as u64)); // (roll back the lexer first)
                Ok(Object::Integer(first_lexeme.to::<i32>()?))
            }
        } else {
            Ok(Object::Null)
        }
    }

    pub fn read_indirect_object(&self, obj_nr: i32) -> Result<IndirectObject> {
        info!("Read ind object"; "#" => obj_nr);
        let xref_entry = self.xref_table.entries[(obj_nr - self.xref_table.first_id as i32) as usize];
        match xref_entry {
            XrefEntry::Free{next_obj_nr: _, gen_nr:_} => Err(Error::FreeObject {obj_nr: obj_nr}),
            XrefEntry::InUse{pos, gen_nr: _} => self.read_indirect_object_from(pos),
        }
    }

    fn read_indirect_object_from(&self, start_pos: usize) -> Result<IndirectObject> {
        let mut lexer = Lexer::new(&self.buf);
        lexer.seek(SeekFrom::Start(start_pos as u64));
        let obj_nr = lexer.next()?.to::<i32>()?;
        let gen_nr = lexer.next()?.to::<i32>()?;
        let obj_literal = lexer.next()?;
        if !obj_literal.equals(b"obj") {
            return Err(Error::UnexpectedToken {pos: lexer.get_pos(), token: obj_literal.as_string(), expected: "obj"});
        }

        let obj = self.read_object(&mut lexer)?;

        info!("Read ind object from"; "Object" => obj.to_string());
        let endobj_literal = lexer.next()?;
        if !endobj_literal.equals(b"endobj") {
            return Err(Error::UnexpectedToken {pos: lexer.get_pos(), token: endobj_literal.as_string(), expected: "endobj"});
        }

        Ok(IndirectObject {
            obj_nr: obj_nr,
            gen_nr: gen_nr,
            object: obj,
        })
    }
}

fn read_file(path: &str) -> Result<Vec<u8>> {
    let mut file  = File::open(path)?;
    let length = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    let mut buf: Vec<u8> = Vec::new();
    buf.resize(length as usize, 0);
    let _ = file.read(&mut buf); // Read entire file into memory

    Ok(buf)
}
