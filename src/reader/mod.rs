pub mod lexer;

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
    root: Object,
    pub trailer: Object, // only the last trailer in the file
    pages_root: Object, // the root of the tree of pages

    buf: Vec<u8>,
}


impl PdfReader {
    pub fn new(path: &str) -> Result<PdfReader> {
        let buf = read_file(path)?;
        let mut pdf_reader = PdfReader {
            startxref: 0,
            xref_table: XrefTable::new(0),
            root: Object::Null,
            trailer: Object::Null,
            pages_root: Object::Null,
            buf: buf,
        };
        let startxref = pdf_reader.read_startxref()?;
        pdf_reader.startxref = startxref;

        let trailer = pdf_reader.read_last_trailer().chain_err(|| "Error reading trailer.")?;
        pdf_reader.trailer = trailer;

        pdf_reader.startxref = startxref;
        pdf_reader.xref_table = pdf_reader.gather_xref().chain_err(|| "Error reading xref table.")?;
        pdf_reader.root = pdf_reader.read_root().chain_err(|| "Error reading root.")?;
        pdf_reader.pages_root = pdf_reader.read_pages().chain_err(|| "Error reading pages.")?;


        println!("XrefTable:\n{:?}", pdf_reader.xref_table);
        Ok(pdf_reader)
    }
    /// `reference` must be an `Object::Reference`
    pub fn dereference(&self, reference: &Object) -> Result<Object> {
        match reference {
            &Object::Reference {obj_nr, gen_nr:_} => {
                Ok(self.read_indirect_object(obj_nr)?.object)
            },
            _ => {
                // Err(ErrorKind::WrongObjectType.into()).chain_err(|| ErrorKind::ExpectedType {expected: "Reference"})
                Err(ErrorKind::WrongObjectType.into())
            }
        }
    }
    pub fn read_indirect_object(&self, obj_nr: i32) -> Result<IndirectObject> {
        let xref_entry = self.xref_table.get(obj_nr as usize)?;
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
        Ok(page)
    }
    /// Find a page looking in the page tree. Return the Object.
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



    fn read_startxref(&mut self) -> Result<usize> {
        let mut lexer = Lexer::new(&self.buf);

        // Find startxref
        lexer.seek(SeekFrom::End(0));
        let _ = lexer.seek_substr_back(b"startxref")?;
        Ok(lexer.next_as::<usize>()?)
    }

    /// Reads xref and trailer at some byte position `start`.
    /// `start` should point to the `xref` keyword of an xref table, or to the start of an xref
    /// stream.
    fn read_xref_and_trailer_at(&self, start: usize) -> Result<(Vec<XrefSection>, Object)> {
        let mut lexer = Lexer::new(&self.buf);
        lexer.seek(SeekFrom::Start(start as u64));

        let next_word = lexer.next()?;
        if next_word.equals(b"xref") {
            // Read classic xref table
            
            let sections = self.read_xref_table(&mut lexer)?;
            // TODO: Read Trailer!
            Ok((sections, Object::Null))
        } else {
            // Read xref stream

            lexer.back()?;
            self.read_xref_stream(&mut lexer)
        }
    }

    fn read_xref_stream(&self, lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Object)> {
        // TODO We receive &mut Lexer, but only read the pos... Consistency!
        let obj = self.read_indirect_object_from(lexer.get_pos()).chain_err(|| "Reading Xref stream")?.object;
        // TODO Finish this function. Not trivial.
        // For now, writing out to see what's in the eventual Xref stream.
        println!("Xref stream obj: {:?}", obj);
        panic!("Exit");
    }
    fn read_xref_table(&self, lexer: &mut Lexer) -> Result<Vec<XrefSection>> {
        let mut sections = Vec::new();
        
        // Keep reading subsections until we hit `trailer`
        while !lexer.peek()?.equals(b"trailer") {
            let start_id = lexer.next_as::<u32>()?;
            let num_ids = lexer.next_as::<u32>()?;

            let mut section = XrefSection::new(start_id);

            for _ in 0..num_ids {
                let w1 = lexer.next()?;
                let w2 = lexer.next()?;
                let w3 = lexer.next()?;
                if w3.equals(b"f") {
                    section.add_free_entry(w1.to::<u32>()?, w2.to::<u16>()?);
                } else if w3.equals(b"n") {
                    section.add_inuse_entry(w1.to::<usize>()?, w2.to::<u16>()?);
                } else {
                    bail!(ErrorKind::UnexpectedLexeme {pos: lexer.get_pos(), lexeme: w3.as_string(), expected: "f or n"});
                }
            }
            sections.push(section);
        }
        Ok(sections)
    }

    /// Gathers all xref sections in the file to an XrefTable.
    /// Agnostic about whether there are xref tables or xref streams. (but haven't thought about
    /// hybrid ones)
    fn gather_xref(&self) -> Result<XrefTable> {
        let num_objects = match self.trailer.dict_get("Size".into()) {
            Ok(&Object::Integer (n)) => n,
            Ok(_) => bail!("Trailer /Size is not Integer."),
            Err(Error (ErrorKind::NotFound {word:_}, _)) => bail!("Trailer /Size not found."),
            Err(_) => bail!("Trailer is not Dictionary {:?}", self.trailer),
        };

        let mut table = XrefTable::new(num_objects as usize);
        
        let (sections, _) = self.read_xref_and_trailer_at(self.startxref)?;
        for section in sections {
            table.add_entries_from(section);
        }

        let mut lexer = Lexer::new(&self.buf);

        let mut next_trailer_start: Option<i32>
            = self.trailer.dict_get("Prev".into()).and_then(|x| Ok(x.unwrap_integer()?)).ok();
        
        while let Some(trailer_start) = next_trailer_start {
            // - jump to next `trailer`
            lexer.seek(SeekFrom::Start(trailer_start as u64));
            // - read that trailer to gather next trailer start and startxref
            let (trailer, startxref) = self.read_trailer_at(&mut lexer)?;
            next_trailer_start = trailer.dict_get("Prev".into())
                .and_then(|x| Ok(x.unwrap_integer()?)).ok();
            // - read xref table
            let (sections, _) = self.read_xref_and_trailer_at(trailer_start as usize)?;
            // TODO trailer start?? not Xref start??
            for section in sections {
                table.add_entries_from(section);
            }
        }
        Ok(table)

    }


    /// Needs to be called before any other functions on the PdfReader
    /// Reads the last trailer in the file
    fn read_last_trailer(&mut self) -> Result<Object> {
        let (_, trailer) = self.read_xref_and_trailer_at(self.startxref)?;
        Ok(trailer)
    }
    /// Returns the trailer dictionary and startxref
    fn read_trailer_at(&self, lexer: &mut Lexer) -> Result<(Object, usize)> {
        // Read trailer
        lexer.next_expect("trailer")?;
        let trailer = self.read_object(lexer)?;
        
        // Read startxref
        lexer.next_expect("startxref")?;

        let startxref = lexer.next_as::<usize>()?;

        Ok((trailer, startxref))
    }

    /// Read the Root/Catalog object
    fn read_root(&self) -> Result<Object> {
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
                    let obj = self.read_object(lexer)?;
                    dictionary.push( (key, obj) );
                } else if delimiter.equals(b">>") {
                    break;
                } else {
                    println!("Dicionary in progress: {:?}", dictionary);
                    bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.as_string(), expected: "/ or >>"});
                }
            }
            // It might just be the dictionary in front of a stream.
            let dict = Object::Dictionary(dictionary.clone());
            if lexer.peek()?.equals(b"stream") {
                lexer.next()?;

                // Get length
                let length_obj = dict.dict_get("Length".into())?;

                let length = self.dereference(length_obj)?.unwrap_integer()?;
                // Read the stream
                let content = lexer.seek(SeekFrom::Current(length as i64));
                // Finish
                lexer.next_expect("endstream")?;

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
            bail!("Can't recognize type. Pos: {}\n\tFirst lexeme: {}\n\tRest:\n{}\n\n\tEnd rest\n",
                  lexer.get_pos(),
                  first_lexeme.as_string(),
                  lexer.read_n(50).as_string());
        }
    }


    fn read_indirect_object_from(&self, start_pos: usize) -> Result<IndirectObject> {
        let mut lexer = Lexer::new(&self.buf);
        lexer.seek(SeekFrom::Start(start_pos as u64));
        let obj_nr = lexer.next()?.to::<i32>()?;
        let gen_nr = lexer.next()?.to::<i32>()?;
        lexer.next_expect("obj")?;

        let obj = self.read_object(&mut lexer)?;

        lexer.next_expect("endobj")?;

        Ok(IndirectObject {
            obj_nr: obj_nr,
            gen_nr: gen_nr,
            object: obj,
        })
    }

}

pub fn read_file(path: &str) -> Result<Vec<u8>> {
    let mut file  = File::open(path)?;
    let length = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    let mut buf: Vec<u8> = Vec::new();
    buf.resize(length as usize, 0);
    let _ = file.read(&mut buf); // Read entire file into memory

    Ok(buf)
}
