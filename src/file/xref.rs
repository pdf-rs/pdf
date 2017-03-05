use err::*;
use std;
use std::fmt::{Debug, Formatter};

///////////////////////////
// Cross-reference table //
///////////////////////////

#[derive(Copy, Clone, Debug)]
pub enum XrefEntry {
    Free {next_obj_nr: u32, gen_nr: u16},
    InUse {pos: usize, gen_nr: u16},
    /// In use and compressed inside an Object Stream
    InStream {stream_obj_nr: u32, index: u16},

}

impl XrefEntry {
    pub fn get_gen_nr(&self) -> u16 {
        match self {
            &XrefEntry::Free {next_obj_nr: _, gen_nr} => gen_nr,
            &XrefEntry::InUse {pos: _, gen_nr} => gen_nr,
            &XrefEntry::InStream {stream_obj_nr: _, index: _} => 0, // TODO I think these always have gen nr 0?
        }
    }
}


/// Runtime lookup table of all objects
pub struct XrefTable {
    // None means that it's not specified, and should result in an error if used
    // Thought: None could also mean Free?
    entries: Vec<Option<XrefEntry>>
}


impl XrefTable {
    pub fn new(num_objects: usize) -> XrefTable {
        let mut entries = Vec::new();
        entries.resize(num_objects, None);
        XrefTable {
            entries: entries,
        }
    }

    pub fn iter<'a>(&'a self) -> ObjectNrIter<'a> {
        ObjectNrIter {
            xref_table: &self,
            obj_nr: -1,
        }
    }

    pub fn get(&self, index: usize) -> Result<XrefEntry> {
        match self.entries[index] {
            Some(entry) => Ok(entry),
            None => bail!("Entry {} in xref table unspecified.", index),
        }
    }
    pub fn num_entries(&self) -> usize {
        self.entries.len()
    }

    pub fn add_entries_from(&mut self, section: XrefSection) {
        for (i, entry) in section.entries.iter().enumerate() {
            // Early return if the entry we have has larger or equal generation number
            let should_be_updated = match self.entries[i].clone() {
                Some(existing_entry) => {
                    if entry.get_gen_nr() <= existing_entry.get_gen_nr() {
                        false
                    } else {
                        true
                    }
                },
                None => true,
            };
            if should_be_updated {
                self.entries[section.first_id as usize + i] = Some(entry.clone());
            }
        }
    }
}

impl Debug for XrefTable {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (i, entry) in self.entries.iter().enumerate() {
            match entry {
                &Some(XrefEntry::Free {next_obj_nr, gen_nr}) => {
                    write!(f, "{:4}: {:010} {:05} f \n", i, next_obj_nr, gen_nr)?
                },
                &Some(XrefEntry::InUse {pos, gen_nr}) => {
                    write!(f, "{:4}: {:010} {:05} n \n", i, pos, gen_nr)?
                },
                &Some(XrefEntry::InStream {stream_obj_nr, index}) => {
                    write!(f, "{:4}: in stream {}, index {}\n", i, stream_obj_nr, index)?
                }
                &None => {
                    write!(f, "{:4}: None!\n", i)?
                }
            }
        }
        Ok(())
    }
}

/// As found in PDF files
#[derive(Debug)]
pub struct XrefSection {
    pub first_id: u32,
    pub entries: Vec<XrefEntry>,
}


impl XrefSection {
    pub fn new(first_id: u32) -> XrefSection {
        XrefSection {
            first_id: first_id,
            entries: Vec::new(),
        }
    }
    pub fn add_free_entry(&mut self, next_obj_nr: u32, gen_nr: u16) {
        self.entries.push(XrefEntry::Free{next_obj_nr: next_obj_nr, gen_nr: gen_nr});
    }
    pub fn add_inuse_entry(&mut self, pos: usize, gen_nr: u16) {
        self.entries.push(XrefEntry::InUse{pos: pos, gen_nr: gen_nr});
    }
}


/// Iterates over the used object numbers in this xref table, skips the free objects.
pub struct ObjectNrIter<'a> {
    xref_table: &'a XrefTable,
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
                Some(XrefEntry::Free {..}) => self.next(),
                Some(_) => Some(self.obj_nr as u32),
                None => self.next(),
            }
        }
    }
}

// read_xref_table
// read_xref_stream
// read_xref_and_trailer_at
