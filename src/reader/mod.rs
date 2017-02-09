pub mod lexer;

use reader::lexer::StringLexer;
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
    root: Dictionary,
    pub trailer: Dictionary, // only the last trailer in the file
    pages_root: Dictionary, // the root of the tree of pages

    buf: Vec<u8>,
}


impl PdfReader {
    pub fn new(path: &str) -> Result<PdfReader> {
        let buf = read_file(path)?;
        let mut pdf_reader = PdfReader {
            startxref: 0,
            xref_table: XrefTable::new(0),
            root: Dictionary::new(),
            trailer: Dictionary::new(),
            pages_root: Dictionary::new(),
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
        Ok(pdf_reader)
    }
    /// Consumes the Object, and returns either the same object, or the object pointed to, if `obj`
    /// is a reference.
    pub fn dereference(&self, obj: Object) -> Result<Object> {
        match obj {
            Object::Reference (id) => {
                Ok(
                    // Recursively dereference...
                    self.dereference(self.read_indirect_object(id.obj_nr)?)?
                )
            },
            _ => {
                Ok(obj)
            }
        }
    }
    pub fn read_indirect_object(&self, obj_nr: u32) -> Result<Object> {
        trace!("Look up in xref table"; "obj_nr" => obj_nr);
        let xref_entry = self.xref_table.get(obj_nr as usize)?; // TODO why usize?
        match xref_entry {
            XrefEntry::Free {next_obj_nr: _, gen_nr:_} => Err(ErrorKind::FreeObject {obj_nr: obj_nr}.into()),
            XrefEntry::InUse {pos, gen_nr: _} => {
                let mut lexer = Lexer::new(&self.buf);
                lexer.seek(SeekFrom::Start(pos as u64));
                let indirect_obj = IndirectObject::parse_from(&mut lexer)?;
                if indirect_obj.id.obj_nr != obj_nr {
                    bail!("xref table is wrong: read indirect obj of wrong obj_nr {} != {}", indirect_obj.id.obj_nr, obj_nr);
                }
                Ok(indirect_obj.object)
            }
            XrefEntry::InStream {stream_obj_nr, index} => {
                let obj_stream = self.read_indirect_object(stream_obj_nr)?.as_stream()?;
                obj_stream.dictionary.expect_type("ObjStm")?;
                Object::parse_from_stream(&obj_stream, index)
            }
        }
    }

    pub fn get_num_pages(&self) -> i32 {

        let result = self.pages_root.get("Count");
        match result {
            Ok(&Object::Integer(n)) => n,
            _ => 0,
        }
    }

    /// Returns Dictionary, with /Type = Page.
    /// page_nr must be smaller than `self.get_num_pages()`
    pub fn get_page_contents(&self, page_nr: i32) -> Result<Dictionary> {
        if page_nr >= self.get_num_pages() {
            return Err(ErrorKind::OutOfBounds.into());
        }
        let page = self.find_page(page_nr)?;
        Ok(page)
    }
    /// Find a page looking in the page tree. Return the Object.
    fn find_page(&self, page_nr: i32) -> Result<Dictionary> {
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
    fn find_page_internal(&self, page_nr: i32, progress: &mut i32, node: &Dictionary ) -> Result<Option<Dictionary>> {
        if *progress > page_nr {
            // Search has already passed the correct one...
            bail!("Search has passed the page nr, without finding the page.");
        }

        if let Ok(&Object::Name(ref t)) = node.get("Type") {
            if *t == "Pages".to_string() { // Intermediate node
                // Number of leaf nodes (pages) in this subtree
                let count = if let &Object::Integer(n) = node.get("Count")? {
                        n
                    } else {
                        bail!("No Count.");
                    };

                // If the target page is a descendant of the intermediate node
                if *progress + count > page_nr {
                    let kids = if let &Object::Array(ref kids) = node.get("Kids")? {
                            kids
                        } else {
                            bail!("No Kids entry in Pages object.");
                        };
                    // Traverse children of node.
                    for kid in kids {
                        let result = self.find_page_internal(page_nr, progress, &self.dereference(kid.clone())?.as_dictionary()?)?;
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

    fn lexer_at(&self, pos: usize) -> Lexer {
        let mut lexer = Lexer::new(&self.buf);
        lexer.seek(SeekFrom::Start (pos as u64));
        lexer
    }

    fn read_startxref(&mut self) -> Result<usize> {
        let mut lexer = Lexer::new(&self.buf);
        lexer.seek(SeekFrom::End(0));
        let _ = lexer.seek_substr_back(b"startxref")?;
        Ok(lexer.next_as::<usize>()?)
    }

    /// Reads xref and trailer at some byte position `start`.
    /// `start` should point to the `xref` keyword of an xref table, or to the start of an xref
    /// stream.
    fn read_xref_and_trailer_at(lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
        let next_word = lexer.next()?;
        if next_word.equals(b"xref") {
            // Read classic xref table
            
            PdfReader::parse_xref_table_and_trailer(lexer)
        } else {
            // Read xref stream

            lexer.back()?;
            PdfReader::parse_xref_stream_and_trailer(lexer)
        }
    }

    fn parse_xref_stream_and_trailer(lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
        let xref_stream = IndirectObject::parse_from(lexer).chain_err(|| "Reading Xref stream")?.object.as_stream()?;

        // Get 'W' as array of integers
        let width = xref_stream.dictionary.get("W")?.clone().as_integer_array()?; // TODO need borrow_as_array etc, or something
        let entry_size = width.iter().fold(0, |x, &y| x + y);
        let num_entries = xref_stream.dictionary.get("Size")?.as_integer()?;

        let indices: Vec<(i32, i32)> = {
            match xref_stream.dictionary.get("Index") {
                Ok(obj) => obj.borrow_integer_array()?,
                Err(_) => vec![0, num_entries],
            }.chunks(2).map(|c| (c[0], c[1])).collect()
            // ^^ TODO panics if odd number of elements - how to handle it?
        };
        
        let (dict, data) = (xref_stream.dictionary, xref_stream.content);
        
        let mut data_left = &data[..];

        let mut sections = Vec::new();
        for (first_id, num_objects) in indices {
            let section = XrefSection::new_from_xref_stream(first_id, num_entries, &width, &mut data_left)?;
            sections.push(section);
        }
        // debug!("Xref stream"; "Sections" => format!("{:?}", sections));

        // TODO Shouldn't be necessary to clone as we don't use xref_stream anymore.
        Ok((sections, dict))
    }

    /// Reads xref table
    fn parse_xref_table_and_trailer(lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
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
        // Read trailer
        lexer.next_expect("trailer")?;
        let trailer = Object::parse_from(lexer)?.as_dictionary()?;
     
        Ok((sections, trailer))

    }

    /// Gathers all xref sections in the file to an XrefTable.
    /// Agnostic about whether there are xref tables or xref streams. (but haven't thought about
    /// hybrid ones)
    fn gather_xref(&self) -> Result<XrefTable> {
        let mut lexer = Lexer::new(&self.buf);
        let num_objects = self.trailer.get("Size")?.as_integer()?;

        let mut table = XrefTable::new(num_objects as usize);

        let mut next_xref_start: Option<i32> = Some(self.startxref as i32);
        
        while let Some(xref_start) = next_xref_start {
            // Jump to next `trailer`
            lexer.seek(SeekFrom::Start(xref_start as u64));
            // Add sections
            let (sections, trailer) = PdfReader::read_xref_and_trailer_at(&mut self.lexer_at(xref_start as usize))?;
            for section in sections {
                table.add_entries_from(section);
            }
            // Find position of eventual next xref & trailer
            next_xref_start = trailer.get("Prev")
                .and_then(|x| Ok(x.as_integer()?)).ok();
        }
        debug!("XREF TABLE"; "table" => format!("{:?}", table));
        Ok(table)

    }


    /// Needs to be called before any other functions on the PdfReader
    /// Reads the last trailer in the file
    fn read_last_trailer(&mut self) -> Result<Dictionary> {
        trace!("-> read_last_trailer");
        let (_, trailer) = PdfReader::read_xref_and_trailer_at(&mut self.lexer_at(self.startxref))?;
        trace!("_ read_last_trailer");
        Ok(trailer)
    }

    /// Read the Root/Catalog object
    fn read_root(&self) -> Result<Dictionary> {
        // TODO Shouldn't have to clone here...
        self.dereference(self.trailer.get("Root")?.clone())?.as_dictionary()
    }

    fn read_pages(&self) -> Result<Dictionary> {
        self.dereference(self.root.get("Pages")?.clone())?.as_dictionary()
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

