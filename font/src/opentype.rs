use std::convert::TryInto;
use crate::{CffFont, TrueTypeFont, BorrowedFont, R, IResultExt};
use nom::{
    number::complete::{be_i16, be_u16, be_i64, be_i32, be_u32},
    multi::count,
    combinator::map,
    bytes::complete::take,
    sequence::tuple
};

pub fn parse_opentype<'a>(data: &'a [u8], idx: u32) -> Box<dyn BorrowedFont<'a> + 'a> {
    let tables = parse_tables(data).get();
    for &(tag, _) in &tables.entries {
        debug!("tag: {:?} ({:?})", tag, std::str::from_utf8(&tag));
    }
    let table = |tag: &[u8; 4]| tables.get(tag);
    
    for &(tag, block) in &tables.entries {
        match &tag {
            b"CFF " => return Box::new(CffFont::parse(block, idx)) as _,
            b"glyf" => {
                return Box::new(TrueTypeFont::parse_glyf(block, tables)) as _
            }
            _ => {}
        }
    }
    panic!("neither CFF nor glyf table found")
}

pub struct Tables<'a> {
    // (tag, data)
    entries: Vec<([u8; 4], &'a [u8])>
}
impl<'a> Tables<'a> {
    pub fn get(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        self.entries.iter().find(|&(t, block)| tag == t).map(|&(tag, block)| block)
    }
}
// (tag, content)
pub fn parse_tables(data: &[u8]) -> R<Tables> {
    let (i, _magic) = take(4usize)(data)?; 
    let (i, num_tables) = be_u16(i)?;
    let (i, _search_range) = be_u16(i)?;
    let (i, _entry_selector) = be_u16(i)?;
    let (i, _range_shift) = be_u16(i)?;
    let (i, entries) = count(
        map(
            tuple((take(4usize), be_u32, be_u32, be_u32)),
            |(tag, _, off, len)| (
                tag.try_into().expect("slice too short"),
                data.get(off as usize .. off as usize + len as usize).expect("out of bounds")
            )
        ),
        num_tables as usize
    )(i)?;
    
    Ok((i, Tables { entries }))
}

pub struct Head {
    pub units_per_em: u16,
    pub index_to_loc_format: i16
}
pub fn parse_head(i: &[u8]) -> R<Head> {
    let (i, major) = be_u16(i)?;
    assert_eq!(major, 1);
    let (i, minor) = be_u16(i)?;
    assert_eq!(minor, 0);
    
    let (i, _revision) = be_i32(i)?;
    let (i, _cksum) = be_u32(i)?;
    let (i, magic) = be_i32(i)?;
    assert_eq!(magic, 0x5F0F3CF5);
    
    let (i, _flags) = be_u16(i)?;
    let (i, units_per_em) = be_u16(i)?;

    let (i, _created) = be_i64(i)?;
    let (i, _modified) = be_i64(i)?;
    
    let (i, _x_min) = be_i16(i)?;
    let (i, _y_min) = be_i16(i)?;
    let (i, _x_max) = be_i16(i)?;
    let (i, _y_max) = be_i16(i)?;
    
    let (i, _mac_style) = be_u16(i)?;
    
    let (i, _lowest_rec_ppem) = be_u16(i)?;
    
    let (i, _font_direction_hint) = be_u16(i)?;
    let (i, index_to_loc_format) = be_i16(i)?;
    let (i, glyph_data_format) = be_u16(i)?;
    assert_eq!(glyph_data_format, 0);
    
    Ok((i, Head {
        units_per_em,
        index_to_loc_format
    }))
}
pub struct Maxp {
    pub num_glyphs: u16
}
pub fn parse_maxp(i: &[u8]) -> R<Maxp> {
    let (i, _version) = be_i32(i)?;
    let (i, num_glyphs) = be_u16(i)?;
    Ok((i, Maxp { num_glyphs }))
}
pub fn parse_loca<'a>(i: &'a [u8], head: &Head, maxp: &Maxp) -> R<'a, Vec<u32>> {
    match head.index_to_loc_format {
        0 => count(map(be_u16, |n| 2 * n as u32), maxp.num_glyphs as usize + 1)(i),
        1 => count(be_u32, maxp.num_glyphs as usize + 1)(i),
        _ => panic!("invalid index_to_loc_format")
    }
}
