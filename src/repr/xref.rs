use err::*;
use std;
use std::fmt::{Debug, Formatter};
use num_traits::PrimInt;

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
            &XrefEntry::InStream {stream_obj_nr: u32, index: u16} => 0, // TODO I think these always have gen nr 0?
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

    pub fn get(&self, index: usize) -> Result<XrefEntry> {
        match self.entries[index] {
            Some(entry) => Ok(entry),
            None => bail!("Entry {} in xref table unspecified.", index),
        }
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
                    write!(f, "{:4}: in stream {}, index {}", i, stream_obj_nr, index)?
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
    /// Takes `&mut &[u8]` so that it can "consume" data as it reads
    pub fn new_from_xref_stream(first_id: i32, num_entries: i32, width: &Vec<i32>, data: &mut &[u8]) -> Result<XrefSection> {
        let mut entries = Vec::new();
        for i in 0..num_entries {
            let _type = XrefSection::read_u64_from_stream(width[0], data);
            let field1 = XrefSection::read_u64_from_stream(width[1], data);
            let field2 = XrefSection::read_u64_from_stream(width[2], data);

            let entry =
            match _type {
                0 => XrefEntry::Free {next_obj_nr: field1 as u32, gen_nr: field2 as u16},
                1 => XrefEntry::InUse {pos: field1 as usize, gen_nr: field2 as u16},
                2 => XrefEntry::InStream {stream_obj_nr: field1 as u32, index: field2 as u16},
                _ => bail!("Reading xref stream, The first field 'type' is {}", _type),
            };
            entries.push(entry);
        }
        Ok(XrefSection {
            first_id: first_id as u32,
            entries: entries,
        })
    }
    /// Helper to read an integer with a certain amount of bits `width` from stream.
    fn read_u64_from_stream(width: i32, data: &mut &[u8]) -> u64 {
        let mut result = 0;
        for i in 0..width {
            let i = width - 1 - i; // (width, 0]
            let c: u8 = data[0];
            *data = &data[1..]; // Consume byte
            result += c as u64 * 256.pow(i as u32);
        }
        result
    }
    pub fn add_free_entry(&mut self, next_obj_nr: u32, gen_nr: u16) {
        self.entries.push(XrefEntry::Free{next_obj_nr: next_obj_nr, gen_nr: gen_nr});
    }
    pub fn add_inuse_entry(&mut self, pos: usize, gen_nr: u16) {
        self.entries.push(XrefEntry::InUse{pos: pos, gen_nr: gen_nr});
    }
}

// read_xref_table
// read_xref_stream
// read_xref_and_trailer_at
