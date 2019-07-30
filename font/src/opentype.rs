#![allow(non_snake_case)]

use std::convert::TryInto;
use std::collections::HashMap;
use crate::{CffFont, TrueTypeFont, BorrowedFont, R, IResultExt};
use crate::parsers::iterator;
use nom::{
    number::complete::{be_u8, be_i16, be_u16, be_i64, be_i32, be_u32},
    multi::{count},
    combinator::map,
    bytes::complete::take,
    sequence::tuple
};
use tuple::T4;

pub fn parse_opentype<'a>(data: &'a [u8], idx: u32) -> Box<dyn BorrowedFont<'a> + 'a> {
    let tables = parse_tables(data).get();
    for &(tag, _) in &tables.entries {
        debug!("tag: {:?} ({:?})", tag, std::str::from_utf8(&tag));
    }
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
        self.entries.iter().find(|&(t, _)| tag == t).map(|&(_, block)| block)
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
pub fn parse_cmap(input: &[u8]) -> R<HashMap<u32, u16>> {
    let (i, _version) = be_u16(input)?;
    let (i, num_tables) = be_u16(i)?;
    
    let offset = iterator(i, tuple((be_u16, be_u16, be_u32))).take(num_tables as usize)
        .filter_map(|entry| match entry {
            (0, _, off) | (3, 10, off) | (3, 1, off) => Some(off),
            _ => None
        })
        .next();
    
    let mut cmap: HashMap<u32, u16> = HashMap::new();
    if let Some(table) = offset.and_then(|off| input.get(off as usize ..)) {
        let (i, format) = be_u16(table)?;
        debug!("cmap format {}", format);
        let (i, len) = be_u16(i)?;
        let (_i, data) = take(len - 4)(i)?; // aleady have 4 header bytes
        match format {
            0 => {
                let (i, _language) = be_u16(data)?;
                for (code, gid) in iterator(i, be_u8).enumerate() {
                    if code != 0 {
                        cmap.insert(code as u32, gid as u16);
                    }
                }
            }
            4 => {
                let (i, _language) = be_u16(data)?;
                let (i, segCountX2) = be_u16(i)?;
                let (i, _searchRange) = be_u16(i)?;
                let (i, _entrySelector) = be_u16(i)?;
                let (i, _rangeShift) = be_u16(i)?;
                let (i, endCode) = take(segCountX2)(i)?;
                let (i, _reservedPad) = be_u16(i)?;
                let (i, startCode) = take(segCountX2)(i)?;
                let (i, idDelta) = take(segCountX2)(i)?;
                let (glyph_data, idRangeOffset) = take(segCountX2)(i)?;
                for (n, T4(start, end, delta, offset)) in T4(
                    iterator(startCode, be_u16),
                    iterator(endCode, be_u16),
                    iterator(idDelta, be_u16),
                    iterator(idRangeOffset, be_u16)
                ).into_iter().enumerate() {
                    if start == 0xFFFF && end == 0xFFFF {
                        break;
                    }
                    if offset == 0 {
                        for c in start ..= end {
                            let gid = delta.wrapping_add(c);
                            cmap.insert(c as u32, gid);
                        }
                    } else {
                        for c in start ..= end {
                            let index = 2 * (n as u16 + (c - start)) + offset - segCountX2;
                            if index as usize > glyph_data.len() - 2 {
                                break;
                            }
                            let (_, gid) = be_u16(&glyph_data[index as usize ..])?;
                            if gid != 0 {
                                let gid = gid.wrapping_add(delta);
                                cmap.insert(c as u32, gid);
                            }
                        }
                    }
                }
            }
            n => unimplemented!("cmap format {}", n),
        }
    }
    Ok((&[], cmap))
}

pub struct Hhea {
    line_gap: i16,
    number_of_hmetrics: u16 
}
pub fn parse_hhea(i: &[u8]) -> R<Hhea> {
    let (i, _majorVersion) = be_u16(i)?;
    let (i, _minorVersion) = be_u16(i)?;
    let (i, _ascender) = be_i16(i)?;
    let (i, _descender) = be_i16(i)?;
    let (i, line_gap) = be_i16(i)?;
    let (i, _advanceWidthMax) = be_u16(i)?;
    let (i, _minLeftSideBearing) = be_i16(i)?;
    let (i, _minRightSideBearing) = be_i16(i)?;
    let (i, _xMaxExtent) = be_i16(i)?;
    let (i, _caretSlopeRise) = be_i16(i)?;
    let (i, _caretSlopeRun) = be_i16(i)?;
    let (i, _caretOffset) = be_i16(i)?;
    let (i, _) = be_i16(i)?;
    let (i, _) = be_i16(i)?;
    let (i, _) = be_i16(i)?;
    let (i, _) = be_i16(i)?;
    
    let (i, _metricDataFormat) = be_i16(i)?;
    let (i, number_of_hmetrics) = be_u16(i)?;
    
    Ok((i, Hhea {
        line_gap,
        number_of_hmetrics
    }))
}
pub struct Hmtx<'a> {
    data: &'a [u8],
    num_metrics: u16,
    num_glyphs: u16,
    last_advance: u16
}
pub struct HMetrics {
    pub advance: u16,
    pub lsb: i16
}
impl<'a> Hmtx<'a> {
    pub fn metrics_for_gid(&self, gid: u16) -> HMetrics {
        assert!(gid < self.num_glyphs);
        if gid < self.num_metrics {
            let index = gid as usize * 4;
            let (advance, lsb) = tuple((be_u16, be_i16))(&self.data[index ..]).get();
            HMetrics { advance, lsb }
        } else {
            let index = self.num_metrics as usize * 2 + gid as usize * 2;
            let lsb = be_i16(&self.data[index ..]).get();
            HMetrics { advance: self.last_advance, lsb }
        }
    }
}
pub fn parse_hmtx<'a>(data: &'a [u8], hhea: &Hhea, maxp: &Maxp) -> Hmtx<'a> {
    let num_metrics = hhea.number_of_hmetrics;
    let last_advance = if num_metrics > 0 {
        be_u16(&data[num_metrics as usize * 4 - 4 ..]).get()
    } else {
        0
    };
    Hmtx {
        data,
        num_metrics,
        num_glyphs: maxp.num_glyphs,
        last_advance
    }
}
