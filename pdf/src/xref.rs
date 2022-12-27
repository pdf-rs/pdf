use std::fmt::{Debug, Formatter};
use crate::error::*;
use crate::object::*;
use crate as pdf;
use datasize::DataSize;

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
    pub fn get_gen_nr(&self) -> GenNr {
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
    pub fn max_field_widths(&self) -> (u64, u64) {
        let mut max_a = 0;
        let mut max_b = 0;
        for &e in &self.entries {
            let (a, b) = match e {
                XRef::Raw { pos, gen_nr } => (pos as u64, gen_nr as u64),
                XRef::Free { next_obj_nr, gen_nr } => (next_obj_nr, gen_nr as u64),
                XRef::Stream { stream_id, index } => (stream_id as u64, index as u64),
                _ => continue
            };
            max_a = max_a.max(a);
            max_b = max_b.max(b);
        }
        (max_a, max_b)
    }

    pub fn add_entries_from(&mut self, section: XRefSection) {
        for (i, &entry) in section.entries() {
            if let Some(dst) = self.entries.get_mut(i) {
                // Early return if the entry we have has larger or equal generation number
                let should_be_updated = match *dst {
                    XRef::Raw { gen_nr: gen, .. } | XRef::Free { gen_nr: gen, .. }
                        => entry.get_gen_nr() > gen,
                    XRef::Stream { .. } | XRef::Invalid
                        => true,
                    x => panic!("found {:?}", x)
                };
                if should_be_updated {
                    *dst = entry;
                }
            }
        }
    }

    pub fn write_stream(&self, size: usize) -> Result<Stream<XRefInfo>> {
        let (max_a, max_b) = self.max_field_widths();
        let a_w = byte_len(max_a);
        let b_w = byte_len(max_b);

        let mut data = Vec::with_capacity((1 + a_w + b_w) * size);
        for &x in self.entries.iter().take(size) {
            let (t, a, b) = match x {
                XRef::Free { next_obj_nr, gen_nr } => (0, next_obj_nr, gen_nr as u64),
                XRef::Raw { pos, gen_nr } => (1, pos as u64, gen_nr as u64),
                XRef::Stream { stream_id, index } => (2, stream_id as u64, index as u64),
                x => panic!("invalid xref entry: {:?}", x)
            };
            data.push(t);
            data.extend_from_slice(&a.to_be_bytes()[8 - a_w ..]);
            data.extend_from_slice(&b.to_be_bytes()[8 - b_w ..]);
        }
        let _info = XRefInfo {
            size: size as u32,
            index: vec![0, size as u32],
            prev: None,
            w: vec![1, a_w, b_w],
        };
        unimplemented!()
        //Ok(Stream::new(info, data).hexencode())
    }
}

fn byte_len(n: u64) -> usize {
    (64 + 8 - 1 - n.leading_zeros()) as usize / 8 + (n == 0) as usize
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
    pub fn add_inuse_entry(&mut self, pos: usize, gen_nr: GenNr) {
        self.entries.push(XRef::Raw{pos, gen_nr});
    }
    pub fn entries(&self) -> impl Iterator<Item=(usize, &XRef)> {
        self.entries.iter().enumerate().map(move |(i, e)| (i + self.first_id as usize, e))
    }
}


#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type = "XRef")]
pub struct XRefInfo {
    // XRefStream fields
    #[pdf(key = "Size")]
    pub size: u32,

    //
    #[pdf(key = "Index", default = "vec![0, size]")]
    /// Array of pairs of integers for each subsection, (first object number, number of entries).
    /// Default value (assumed when None): `(0, self.size)`.
    pub index: Vec<u32>,

    #[pdf(key = "Prev")]
    prev: Option<i32>,

    #[pdf(key = "W")]
    pub w: Vec<usize>,
}

// read_xref_table
// read_xref_stream
// read_xref_and_trailer_at
