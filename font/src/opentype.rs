use crate::{CffFont, TrueTypeFont, BorrowedFont};
use std::error::Error;
use std::convert::TryInto;

pub fn parse_opentype<'a>(data: &'a [u8], idx: u32) -> Box<dyn BorrowedFont<'a> + 'a> {
    for (header, block) in tables(data) {
        debug!("header: {:?} ({:?})", header, std::str::from_utf8(&header));
        match &header {
            b"CFF " => return Box::new(CffFont::parse(block, idx)) as _,
            b"glyf" => return Box::new(TrueTypeFont::parse(data, idx)) as _,
            _ => {}
        }
    }
    panic!("neither CFF nor glyf table found")
}

fn find_table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let num_tables = u16::from_be_bytes(data[4 .. 6].try_into().unwrap()) as usize;
    for i in 0 .. num_tables {
        let loc = 12 + 16 * i;
        let entry = &data[loc .. loc + 4];
        if entry == tag {
            let offset = u32::from_be_bytes(data[loc + 8 .. loc + 12].try_into().unwrap()) as usize;
            return Some(data.get(offset ..).unwrap());
        }
    }
    None
}

// (header, content)
fn tables(data: &[u8]) -> impl Iterator<Item=([u8; 4], &[u8])> {
    let num_tables = u16::from_be_bytes(data[4 .. 6].try_into().unwrap()) as usize;
    data[12 ..].chunks_exact(16).map(move |chunk| {
        let entry: [u8; 4] = chunk[.. 4].try_into().unwrap();
        let offset = u32::from_be_bytes(chunk[8 .. 12].try_into().unwrap()) as usize;
        (entry, data.get(offset ..).unwrap())
    })
}
