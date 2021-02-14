use std::fmt::{Debug, Formatter};
use std::io::Write;
use crate::error::*;
use crate::object::*;

///////////////////////////
// Cross-reference table //
///////////////////////////

#[derive(Copy, Clone, Debug)]
pub enum XRef {
    /// Not currently used.
    Free {
        next_obj_nr: ObjNr,
        gen_nr: GenNr
    },

    /// In use.
    Raw {
        pos: usize,
        gen_nr: GenNr
    },
    /// In use and compressed inside an Object Stream
    Stream {
        stream_id: ObjNr,
        index: usize,
    },
    
    Promised,
    
    Invalid
}

impl XRef {
    pub fn get_gen_nr(&self) -> u16 {
        match *self {
            XRef::Free {gen_nr, ..}
            | XRef::Raw {gen_nr, ..} => gen_nr,
            XRef::Stream { .. } => 0, // TODO I think these always have gen nr 0?
            _ => panic!()
        }
    }
}


/// Runtime lookup table of all objects
#[derive(Clone)]
pub struct XRefTable {
    // None means that it's not specified, and should result in an error if used
    // Thought: None could also mean Free?
    entries: Vec<XRef>
}


impl XRefTable {
    pub fn new(num_objects: ObjNr) -> XRefTable {
        let mut entries = Vec::new();
        entries.resize(num_objects as usize, XRef::Invalid);
        XRefTable {
            entries,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item=u32> + '_ {
        self.entries.iter().enumerate()
            .filter(|(_, xref)| matches!(xref, XRef::Raw { .. } | XRef::Stream { .. } ))
            .map(|(i, _)| i as u32)
    }

    pub fn get(&self, id: ObjNr) -> Result<XRef> {
        match self.entries.get(id as usize) {
            Some(&entry) => Ok(entry),
            None => Err(PdfError::UnspecifiedXRefEntry {id}),
        }
    }
    pub fn set(&mut self, id: ObjNr, r: XRef) {
        self.entries[id as usize] = r;
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn push(&mut self, new_entry: XRef) {
        self.entries.push(new_entry);
    }
    pub fn num_entries(&self) -> usize {
        self.entries.len()
    }
    pub fn max_pos_and_gen(&self) -> (u64, GenNr) {
        let mut max_pos = 0;
        let mut max_gen = 0;
        for &e in &self.entries {
            match e {
                XRef::Raw { pos, gen_nr } => {
                    max_pos = max_pos.max(pos as u64);
                    max_gen = max_gen.max(gen_nr);
                }
                XRef::Free { next_obj_nr, gen_nr } => {
                    max_pos = max_pos.max(next_obj_nr);
                    max_gen = max_gen.max(gen_nr);
                }
                _ => ()
            }
        }
        (max_pos, max_gen)
    }

    pub fn add_entries_from(&mut self, section: XRefSection) {
        for (i, entry) in section.entries() {
            // Early return if the entry we have has larger or equal generation number
            let should_be_updated = match self.entries[i] {
                XRef::Raw { gen_nr: gen, .. } | XRef::Free { gen_nr: gen, .. }
                    => entry.get_gen_nr() > gen,
                XRef::Stream { .. } | XRef::Invalid
                    => true,
                x => panic!("found {:?}", x)
            };
            let dst = &mut self.entries[i];
            if should_be_updated {
                *dst = *entry;
            }
        }
    }

    pub fn write_old_format(&self, out: &mut Vec<u8>) {
        for &x in self.entries.iter() {
            let (n, g, f) = match x {
                XRef::Free { next_obj_nr, gen_nr } => (next_obj_nr, gen_nr, 'f'),
                XRef::Raw { pos, gen_nr } => (pos as u64, gen_nr, 'n'),
                x => panic!("invalid xref entry: {:?}", x)
            };
            write!(out, "{:010x} {:05x} {} \n", n, g, f).unwrap();
        }
    }
    /*
    pub fn write_stream(&self) -> Vec<u8> {
        let (max_pos, max_gen) = self.max_pos_and_gen();
        let off_w = len_base16(max_pos);
        let id_w = len_base16(self.len() as u64);
        let gen_w = len_base16(max_gen as _);
        unimplemented!()
    }
    */
}

fn len_base16(n: u64) -> u32 {
    (67 - n.leading_zeros()) / 4 + (n == 0) as u32
}

impl Debug for XRefTable {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (i, entry) in self.entries.iter().enumerate() {
            match *entry {
                XRef::Free {next_obj_nr, gen_nr} => {
                    writeln!(f, "{:4}: {:010} {:05} f", i, next_obj_nr, gen_nr)?
                },
                XRef::Raw {pos, gen_nr} => {
                    writeln!(f, "{:4}: {:010} {:05} n", i, pos, gen_nr)?
                },
                XRef::Stream {stream_id, index} => {
                    writeln!(f, "{:4}: in stream {}, index {}", i, stream_id, index)?
                },
                XRef::Promised => {
                    writeln!(f, "{:4}: Promised?", i)?
                },
                XRef::Invalid => {
                    writeln!(f, "{:4}: Invalid!", i)?
                }
            }
        }
        Ok(())
    }
}

/// As found in PDF files
#[derive(Debug)]
pub struct XRefSection {
    pub first_id: u32,
    pub entries: Vec<XRef>,
}


impl XRefSection {
    pub fn new(first_id: u32) -> XRefSection {
        XRefSection {
            first_id,
            entries: Vec::new(),
        }
    }
    pub fn add_free_entry(&mut self, next_obj_nr: ObjNr, gen_nr: GenNr) {
        self.entries.push(XRef::Free{next_obj_nr, gen_nr});
    }
    pub fn add_inuse_entry(&mut self, pos: usize, gen_nr: u16) {
        self.entries.push(XRef::Raw{pos, gen_nr});
    }
    pub fn entries(&self) -> impl Iterator<Item=(usize, &XRef)> {
        self.entries.iter().enumerate().map(move |(i, e)| (i + self.first_id as usize, e))
    }
}


// read_xref_table
// read_xref_stream
// read_xref_and_trailer_at
