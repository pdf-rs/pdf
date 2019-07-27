use crate::CffFont;
use std::error::Error;
use std::convert::TryInto;

pub fn parse_opentype(data: &[u8], idx: u32) -> CffFont {
    let cff_data = find_table(data, b"CFF ").expect("no CFF table");
    CffFont::parse(cff_data, idx)
}

fn find_table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let num_tables = u16::from_be_bytes(data[4 .. 6].try_into().unwrap()) as usize;
    for i in 0 .. num_tables {
        let loc = 12 + 16 * i;
        if &data[loc .. loc + 4] == tag {
            let offset = u32::from_be_bytes(data[loc + 8 .. loc + 12].try_into().unwrap()) as usize;
            return Some(data.get(offset ..).unwrap());
        }
    }
    None
}
