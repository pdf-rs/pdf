mod lexer;

use repr::*;

use err::*;

use self::lexer::Lexer;
use std::vec::Vec;
use std::io::SeekFrom;
use std::io::Seek;
use std::io::Read;
use std::fs::File;

pub struct PdfReader {
    // Contents
    startxref: usize,
    xref_table: XrefTable,
    pub trailer: Object,
    root: Object,
    pages_root: Object, // the root of the tree of pages

    buf: Vec<u8>,
}


impl PdfReader {
    pub fn new(path: &str) -> Result<PdfReader> {
        let buf = read_file(path)?;
        let mut pdf_reader = PdfReader {
            startxref: 0,
            xref_table: XrefTable::new(0),
            trailer: Object::Null,
            root: Object::Null,
            pages_root: Object::Null,
            buf: buf,
        };
        pdf_reader.startxref = pdf_reader.read_startxref().chain_err(|| "Error reading startxref.")?;
        let start = pdf_reader.startxref;
        pdf_reader.xref_table = pdf_reader.read_xref(start).chain_err(|| "Error reading xref table.")?;
        pdf_reader.trailer = pdf_reader.read_trailer().chain_err(|| "Error reading trailer.")?;
        pdf_reader.root = pdf_reader.read_root().chain_err(|| "Error reading root.")?;
        pdf_reader.pages_root = pdf_reader.read_pages().chain_err(|| "Error reading pages.")?;

        Ok(pdf_reader)
    }
    /// `reference` must be an `Object::Reference`
    pub fn dereference(&self, reference: &Object) -> Result<Object> {
        match reference {
            &Object::Reference {obj_nr, gen_nr:_} => {
                Ok(self.read_indirect_object(obj_nr)?.object)
            },
            _ => {
                Err(ErrorKind::WrongObjectType.into())
            }
        }
    }
    pub fn read_indirect_object(&self, obj_nr: i32) -> Result<IndirectObject> {
        info!("Read ind object"; "#" => obj_nr);
        let xref_entry = self.xref_table.entries[(obj_nr - self.xref_table.first_id as i32) as usize];
        match xref_entry {
            XrefEntry::Free{next_obj_nr: _, gen_nr:_} => Err(ErrorKind::FreeObject {obj_nr: obj_nr}.into()),
            XrefEntry::InUse{pos, gen_nr: _} => self.read_indirect_object_from(pos),
        }
    }

    pub fn get_num_pages(&self) -> i32 {
        let result = self.pages_root.dict_get("Count".into());
        match result {
            Ok(&Object::Integer(n)) => n,
            _ => 0,
        }
    }

    /// Returns Dictionary, with /Type = Page.
    /// page_nr must be smaller than `self.get_num_pages()`
    pub fn get_page_contents(&self, page_nr: i32) -> Result<Object> {
        if page_nr >= self.get_num_pages() {
            return Err(ErrorKind::OutOfBounds.into());
        }
        let page = self.find_page(page_nr)?;
        // self.dereference(&page)
        Ok(page)
    }
    /// Returns a Reference.
    fn find_page(&self, page_nr: i32) -> Result<Object> {
        let result = self.find_page_internal(page_nr, &mut 0, &self.pages_root)?;
        match result {
            Some(page) => Ok(page),
            None => bail!("Failed to find page"),
        }
    }

    /// `page_nr`: the number of the wanted page
    /// `progress` is the page number of the first leaf of the current tree
    /// A recursive process which returns a page if found, and in any case, the number of pages
    /// traversed (i32)
    fn find_page_internal(&self, page_nr: i32, progress: &mut i32, node: &Object ) -> Result<Option<Object>> {
        if *progress > page_nr {
            // Search has already passed the correct one...
            bail!("Search has passed the page nr, without finding the page.");
        }

        if let Ok(&Object::Name(ref t)) = node.dict_get("Type".into()) {
            if *t == "Pages".to_string() { // Intermediate node
                // Number of leaf nodes (pages) in this subtree
                let count = if let &Object::Integer(n) = node.dict_get("Count".into())? {
                        n
                    } else {
                        bail!("No Count.");
                    };

                // If the target page is a descendant of the intermediate node
                if *progress + count > page_nr {
                    let kids = if let &Object::Array(ref kids) = node.dict_get("Kids".into())? {
                            kids
                        } else {
                            bail!("No Kids entry in Pages object.");
                        };
                    // Traverse children of node.
                    for kid in kids {
                        let result = self.find_page_internal(page_nr, progress, &self.dereference(&kid)?)?;
                        match result {
                            Some(found_page) => return Ok(Some(found_page)),
                            None => {},
                        };
                    }
                    Ok(None)
                } else {
                    Ok(None)
                }
            } else if *t == "Page".to_string() { // Leaf node
                if page_nr == *progress {
                    Ok(Some(node.clone()))
                } else {
                    *progress += 1;
                    Ok(None)
                }
            } else {
                Err("Dictionary is not of Type Page nor Pages".into())
            }
        } else {
            Err("Dictionary has no Type attribute".into())
        }
    }


    //////////////
    // Private: //
    //////////////

    fn read_startxref(&mut self) -> Result<usize> {
        let mut lexer = Lexer::new(&self.buf);

        // Find startxref
        lexer.seek(SeekFrom::End(0));
        let _ = lexer.seek_substr_back(b"startxref")?;
        Ok(lexer.next_as::<usize>()?)
    }

    fn read_xref(&mut self, start: usize) -> Result<XrefTable> {
        let mut lexer = Lexer::new(&self.buf);

        // Read xref
        lexer.seek(SeekFrom::Start(start as u64));
        let word = lexer.next()?;
        if word.as_str() != "xref" {
            bail!(ErrorKind::InvalidXref{pos: lexer.get_pos()});
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
    fn read_trailer(&mut self) -> Result<Object> {
        let mut lexer = Lexer::new(&self.buf);

        // Find startxref
        lexer.seek(SeekFrom::End(0));
        let _ = lexer.seek_substr_back(b"startxref")?;
        self.startxref = lexer.next_as::<usize>()?;

        // Find trailer start
        let _ = lexer.seek_substr_back(b"trailer")?;
        Ok(self.read_object(&mut lexer)?)
    }

    fn read_root(&self) -> Result<Object> {
        // Read the Root/Catalog object
        self.dereference(self.trailer.dict_get("Root".to_string())?)
    }

    fn read_pages(&self) -> Result<Object> {
        self.dereference(self.root.dict_get("Pages".to_string())?)
    }

    /// Reads object starting at where the `Lexer` is currently at.
    // TODO: Notice how sometimes we peek(), and in one branch we do next() in order to move
    // forward. Consider having a back() instead of next()?
    fn read_object(&self, lexer: &mut Lexer) -> Result<Object> {
        let first_lexeme = lexer.next()?;

        if first_lexeme.equals(b"<<") {
            let mut dictionary = Vec::new();
            loop {
                // Expect a Name (and Object) or the '>>' delimiter
                let delimiter = lexer.next()?;
                if delimiter.equals(b"/") {
                    let key = lexer.next()?.as_string();
                    debug!("READ KEY"; "Key" => key);
                    let obj = self.read_object(lexer)?;
                    dictionary.push( (key, obj) );
                } else if delimiter.equals(b">>") {
                    break;
                } else {
                    println!("Dicionary in progress: {:?}", dictionary);
                    bail!(ErrorKind::UnexpectedToken{ pos: lexer.get_pos(), token: delimiter.as_string(), expected: "/ or >>"});
                }
            }
            // It might just be the dictionary in front of a stream.
            let dict = Object::Dictionary(dictionary.clone());
            if lexer.peek()?.equals(b"stream") {
                lexer.next()?;

                // Get length
                let length_obj = dict.dict_get(String::from("Length"))?;

                let length = // TODO How to shorten?
                    if let &Object::Reference{ obj_nr, gen_nr:_ } = length_obj {
                        if let Object::Integer(length) = self.read_indirect_object(obj_nr)?.object {
                            length
                        } else {
                            // Expected integer
                            bail!(ErrorKind::UnexpectedType{ pos: lexer.get_pos()});
                        }
                    } else {
                        // Expected reference.
                        bail!(ErrorKind::UnexpectedType{ pos: lexer.get_pos()})
                    };
                // Read the stream
                let content = lexer.seek(SeekFrom::Current(length as i64));
                // Finish
                let endstream_literal = lexer.next()?;
                if !endstream_literal.equals(b"endstream") {
                    bail!(ErrorKind::UnexpectedToken {pos: lexer.get_pos(), token: endstream_literal.as_string(), expected: "endstream"} );
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
            // May be Integer or Reference

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
                    // We are probably in an array of numbers - it's not a reference anyway
                    lexer.seek(SeekFrom::Start(pos_bk as u64)); // (roll back the lexer first)
                    Ok(Object::Integer(first_lexeme.to::<i32>()?))
                }
            } else {
                // It is but a number
                lexer.seek(SeekFrom::Start(pos_bk as u64)); // (roll back the lexer first)
                Ok(Object::Integer(first_lexeme.to::<i32>()?))
            }
        } else if first_lexeme.equals(b"/") {
            // Name
            let s = lexer.next()?.as_string();
            Ok(Object::Name(s))
        } else if first_lexeme.equals(b"[") {
            let mut array = Vec::new();
            // Array
            loop {
                let element = self.read_object(lexer)?;
                array.push(element);

                // Exit if closing delimiter
                if lexer.peek()?.equals(b"]") {
                    break;
                }
            }
            lexer.next()?; // Move beyond closing delimiter

            Ok(Object::Array (array))
        } else {
            bail!("Can't recognize type.");
        }
    }


    fn read_indirect_object_from(&self, start_pos: usize) -> Result<IndirectObject> {
        let mut lexer = Lexer::new(&self.buf);
        lexer.seek(SeekFrom::Start(start_pos as u64));
        let obj_nr = lexer.next()?.to::<i32>()?;
        let gen_nr = lexer.next()?.to::<i32>()?;
        let obj_literal = lexer.next()?;
        if !obj_literal.equals(b"obj") {
            bail!(ErrorKind::UnexpectedToken {pos: lexer.get_pos(), token: obj_literal.as_string(), expected: "obj"});
        }

        let obj = self.read_object(&mut lexer)?;
        println!("Read indirect obj: {:?}", obj);

        let endobj_literal = lexer.next()?;
        if !endobj_literal.equals(b"endobj") {
            bail!(ErrorKind::UnexpectedToken {pos: lexer.get_pos(), token: endobj_literal.as_string(), expected: "endobj"});
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
