use err::*;
use std;
use std::fmt::{Debug, Formatter};
use object::*;

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
}

impl XRef {
    pub fn get_gen_nr(&self) -> u16 {
        match *self {
            XRef::Free {gen_nr, ..}
            | XRef::Raw {gen_nr, ..} => gen_nr,
            XRef::Stream { .. } => 0, // TODO I think these always have gen nr 0?
        }
    }
}


/// Runtime lookup table of all objects
pub struct XRefTable {
    // None means that it's not specified, and should result in an error if used
    // Thought: None could also mean Free?
    entries: Vec<Option<XRef>>
}


impl XRefTable {
    pub fn new(num_objects: ObjNr) -> XRefTable {
        let mut entries = Vec::new();
        entries.resize(num_objects as usize, None);
        XRefTable {
            entries: entries,
        }
    }

    pub fn iter(&self) -> ObjectNrIter {
        ObjectNrIter {
            xref_table: self,
            obj_nr: -1,
        }
    }

    pub fn get(&self, id: ObjNr) -> Result<XRef> {
        match self.entries[id as usize] {
            Some(entry) => Ok(entry),
            None => bail!("Entry {} in xref table unspecified.", id),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn push(&mut self, new_entry: XRef) {
        self.entries.push(Some(new_entry));
    }
    pub fn num_entries(&self) -> usize {
        self.entries.len()
    }

    pub fn add_entries_from(&mut self, section: XRefSection) {
        for (i, entry) in section.entries.iter().enumerate() {
            // Early return if the entry we have has larger or equal generation number
            let should_be_updated = match self.entries[i] {
                Some(existing_entry) => {
                    !(entry.get_gen_nr() <= existing_entry.get_gen_nr())
                },
                None => true,
            };
            if should_be_updated {
                self.entries[section.first_id as usize + i] = Some(*entry);
            }
        }
    }
}

impl Debug for XRefTable {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (i, entry) in self.entries.iter().enumerate() {
            match *entry {
                Some(XRef::Free {next_obj_nr, gen_nr}) => {
                    write!(f, "{:4}: {:010} {:05} f \n", i, next_obj_nr, gen_nr)?
                },
                Some(XRef::Raw {pos, gen_nr}) => {
                    write!(f, "{:4}: {:010} {:05} n \n", i, pos, gen_nr)?
                },
                Some(XRef::Stream {stream_id, index}) => {
                    write!(f, "{:4}: in stream {}, index {}\n", i, stream_id, index)?
                }
                None => {
                    write!(f, "{:4}: None!\n", i)?
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
            first_id: first_id,
            entries: Vec::new(),
        }
    }
    pub fn add_free_entry(&mut self, next_obj_nr: ObjNr, gen_nr: GenNr) {
        self.entries.push(XRef::Free{next_obj_nr: next_obj_nr, gen_nr: gen_nr});
    }
    pub fn add_inuse_entry(&mut self, pos: usize, gen_nr: u16) {
        self.entries.push(XRef::Raw{pos: pos, gen_nr: gen_nr});
    }
}


/// Iterates over the used object numbers in this xref table, skips the free objects.
pub struct ObjectNrIter<'a> {
    xref_table: &'a XRefTable,
    obj_nr: i64,
}

impl<'a> Iterator for ObjectNrIter<'a> {
    type Item = u32;
    /// Item = (object number, xref entry)
    fn next(&mut self) -> Option<u32> {
        self.obj_nr += 1;
        if self.obj_nr >= self.xref_table.num_entries() as i64 {
            None
        } else {
            match self.xref_table.entries[self.obj_nr as usize] {
                Some(XRef::Free {..}) | None => self.next(),
                Some(_) => Some(self.obj_nr as u32),
            }
        }
    }
}

// read_xref_table
// read_xref_stream
// read_xref_and_trailer_at
