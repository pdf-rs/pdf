use err::*;

use file::lexer::Lexer;
use file::{Dictionary, XrefTable, Primitive, XrefEntry, XrefSection, ObjectNrIter, ObjectId};
use std::vec::Vec;
use std::io::SeekFrom;
use std::io::Seek;
use std::io::Read;
use std::fs::File;
use std::iter::Iterator;

pub struct Reader {
    // Contents
    startxref: usize,
    xref_table: XrefTable,
    root: Dictionary,
    pub trailer: Dictionary, // only the last trailer in the file
    pages_root: Dictionary, // the root of the tree of pages

    buf: Vec<u8>,
}


impl Reader {
    pub fn from_path(path: &str) -> Result<Reader> {
        let buf = read_file(path)?;
        Reader::new(buf)
    }
    pub fn new(data: Vec<u8>) -> Result<Reader> {
        let mut pdf_reader = Reader {
            startxref: 0,
            xref_table: XrefTable::new(0),
            root: Dictionary::default(),
            trailer: Dictionary::default(),
            pages_root: Dictionary::default(),
            buf: data,
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

    pub fn data(&self) -> &[u8] {
        &self.buf
    }
    
    pub fn objects(&self) -> ObjectIter {
        ObjectIter {
            reader: self,
            obj_nr_iter: self.xref_table.iter(),
        }
    }

    pub fn get_xref_table(&self) -> &XrefTable {
        &self.xref_table
    }

    /// If `obj` is a Reference: reads the indirect object it refers to
    /// Else: Returns a clone of the object.
    // TODO: It shouldn't have to clone..
    pub fn dereference(&self, obj: &Primitive) -> Result<Primitive> {
        match *obj {
            Primitive::Reference (ref id) => {
                self.read_indirect_object(id.obj_nr)
            },
            _ => {
                Ok(obj.clone())
            }
        }
    }

    pub fn read_indirect_object(&self, obj_nr: u32) -> Result<Primitive> {
        let xref_entry = self.xref_table.get(obj_nr as usize)?; // TODO why usize?
        match xref_entry {
            XrefEntry::Free { .. } => Err(ErrorKind::FreeObject {obj_nr: obj_nr}.into()),
            XrefEntry::InUse {pos, ..} => {
                let mut lexer = Lexer::new(&self.buf);
                lexer.set_pos(pos as usize);
                let indirect_obj = self.parse_indirect_object(&mut lexer)?;
                if indirect_obj.id.obj_nr != obj_nr {
                    bail!("xref table is wrong: read indirect obj of wrong obj_nr {} != {}", indirect_obj.id.obj_nr, obj_nr);
                }
                Ok(indirect_obj.object)
            }
            XrefEntry::InStream {stream_obj_nr, index} => {
                let obj_stream = self.read_indirect_object(stream_obj_nr)?.as_stream()?;
                obj_stream.dictionary.expect_type("ObjStm")?;
                self.parse_object_from_stream(&obj_stream, index)
            }
        }
    }

    fn lexer_at(&self, pos: usize) -> Lexer {
        let mut lexer = Lexer::new(&self.buf);
        lexer.set_pos(pos as usize);
        lexer
    }

    fn read_startxref(&mut self) -> Result<usize> {
        let mut lexer = Lexer::new(&self.buf);
        lexer.set_pos_from_end(0);
        let _ = lexer.seek_substr_back(b"startxref")?;
        Ok(lexer.next_as::<usize>()?)
    }

    /// Reads xref and trailer at some byte position `start`.
    /// `start` should point to the `xref` keyword of an xref table, or to the start of an xref
    /// stream.
    fn read_xref_and_trailer_at(&self, lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
        let next_word = lexer.next()?;
        if next_word.equals(b"xref") {
            // Read classic xref table
            
            self.parse_xref_table_and_trailer(lexer)
        } else {
            // Read xref stream

            lexer.back()?;
            self.parse_xref_stream_and_trailer(lexer)
        }
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
            lexer.set_pos(xref_start as usize);
            // Add sections
            let (sections, trailer) = self.read_xref_and_trailer_at(&mut self.lexer_at(xref_start as usize))?;
            for section in sections {
                table.add_entries_from(section);
            }
            // Find position of eventual next xref & trailer
            next_xref_start = trailer.get("Prev")
                .and_then(|x| Ok(x.as_integer()?)).ok();
        }
        Ok(table)

    }


    /// Needs to be called before any other functions on the Reader
    /// Reads the last trailer in the file
    fn read_last_trailer(&mut self) -> Result<Dictionary> {
        let (_, trailer) = self.read_xref_and_trailer_at(&mut self.lexer_at(self.startxref))?;
        Ok(trailer)
    }
}


pub struct ObjectIter<'a> {
    reader: &'a Reader,
    obj_nr_iter: ObjectNrIter<'a>,
}

impl<'a> Iterator for ObjectIter<'a> {
    type Item = Result<(ObjectId, Primitive)>;
    fn next(&mut self) -> Option<Result<(ObjectId, Primitive)>> {
        match self.obj_nr_iter.next() {
            Some(obj_nr) => {
                let id = ObjectId {obj_nr: obj_nr, gen_nr: 0}; // TODO Get the actual gen nr
                Some(self.reader.read_indirect_object(obj_nr).map(|obj| (id, obj)))
             }
            None => None,
        }
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

