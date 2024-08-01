use crate::error::*;
use crate::parser::lexer::Lexer;
use crate::xref::{XRef, XRefSection, XRefInfo};
use crate::primitive::{Primitive, Dictionary};
use crate::object::*;
use crate::parser::{parse_with_lexer, ParseFlags};
use crate::parser::parse_object::{parse_indirect_stream};
use std::convert::TryInto;

// Just the part of Parser which reads xref sections from xref stream.
/// Takes `&mut &[u8]` so that it can "consume" data as it reads
fn parse_xref_section_from_stream(first_id: u32, mut num_entries: usize, width: &[usize], data: &mut &[u8], resolve: &impl Resolve) -> Result<XRefSection> {
    let mut entries = Vec::new();
    let [w0, w1, w2]: [usize; 3] = width.try_into().map_err(|_| other!("invalid xref length array"))?;
    if num_entries * (w0 + w1 + w2) > data.len() {
        if resolve.options().allow_xref_error {
            warn!("not enough xref data. truncating.");
            num_entries = data.len() / (w0 + w1 + w2);
        } else {
            bail!("not enough xref data");
        }
    }
    for _ in 0..num_entries {
        // println!("{:?}", &data[.. width.iter().map(|&i| i as usize).sum()]);
         // TODO Check if width[i] are 0. Use default values from the PDF references.
        let _type = if w0 == 0 {
            1
        } else {
            read_u64_from_stream(w0, data)?
        };
        let field1 = read_u64_from_stream(w1, data)?;
        let field2 = read_u64_from_stream(w2, data)?;

        let entry =
        match _type {
            0 => XRef::Free {next_obj_nr: field1 as ObjNr, gen_nr: field2 as GenNr},
            1 => XRef::Raw {pos: field1 as usize, gen_nr: field2 as GenNr},
            2 => XRef::Stream {stream_id: field1 as ObjNr, index: field2 as usize},
            _ => return Err(PdfError::XRefStreamType {found: _type}), // TODO: Should actually just be seen as a reference to the null object
        };
        entries.push(entry);
    }
    Ok(XRefSection {
        first_id,
        entries,
    })
}
/// Helper to read an integer with a certain amount of bytes `width` from stream.
fn read_u64_from_stream(width: usize, data: &mut &[u8]) -> Result<u64> {
    if width > std::mem::size_of::<u64>() {
        return Err(PdfError::Other { msg: format!("xref stream entry has invalid width {}", width) });
    }
    if width > data.len() {
        return Err(PdfError::Other { msg: format!("xref stream entry has width {} but only {} bytes left to read", width, data.len()) });
    }
    let mut result = 0;
    for i in (0..width).rev() {
        let base = 8 * i; // (width, 0]
        let c: u8 = data[0];
        *data = &data[1..]; // Consume byte
        result += u64::from(c) << base;
    }
    Ok(result)
}


/// Reads xref sections (from stream) and trailer starting at the position of the Lexer.
pub fn parse_xref_stream_and_trailer(lexer: &mut Lexer, resolve: &impl Resolve) -> Result<(Vec<XRefSection>, Dictionary)> {
    let xref_stream = t!(parse_indirect_stream(lexer, resolve, None)).1;
    let trailer = if t!(lexer.next()) == "trailer" {
        let trailer = t!(parse_with_lexer(lexer, resolve, ParseFlags::DICT));
        t!(trailer.into_dictionary())
    } else {
        xref_stream.info.clone()
    };

    let xref_stream = t!(Stream::<XRefInfo>::from_primitive(Primitive::Stream(xref_stream), resolve));
    let mut data_left = &*t!(xref_stream.data(resolve));
    
    let width = &xref_stream.w;

    let index = &xref_stream.index;
    
    if index.len() % 2 != 0 {
        return Err(PdfError::Other { msg: format!("xref stream has {} elements which is not an even number", index.len()) });
    }

    let mut sections = Vec::new();
    for (first_id, num_objects) in index.chunks_exact(2).map(|c| (c[0], c[1])) {
        let section = t!(parse_xref_section_from_stream(first_id, num_objects as usize, width, &mut data_left, resolve));
        sections.push(section);
    }

    Ok((sections, trailer))
}


/// Reads xref sections (from table) and trailer starting at the position of the Lexer.
pub fn parse_xref_table_and_trailer(lexer: &mut Lexer, resolve: &impl Resolve) -> Result<(Vec<XRefSection>, Dictionary)> {
    let mut sections = Vec::new();
    
    // Keep reading subsections until we hit `trailer`
    while lexer.peek()? != "trailer" {
        let start_id = t!(lexer.next_as::<u32>());
        let num_ids = t!(lexer.next_as::<u32>());

        let mut section = XRefSection::new(start_id);

        for i in 0..num_ids {
            let w1 = t!(lexer.next());
            if w1 == "trailer" {
                return Err(PdfError::Other { msg: format!("xref table declares {} entries, but only {} follow.", num_ids, i) });
            }
            let w2 = t!(lexer.next());
            let w3 = t!(lexer.next());
            if w3 == "f" {
                section.add_free_entry(t!(w1.to::<ObjNr>()), t!(w2.to::<GenNr>()));
            } else if w3 == "n" {
                section.add_inuse_entry(t!(w1.to::<usize>()), t!(w2.to::<GenNr>()));
            } else {
                return Err(PdfError::UnexpectedLexeme {pos: lexer.get_pos(), lexeme: w3.to_string(), expected: "f or n"});
            }
        }
        sections.push(section);
    }

    t!(lexer.next_expect("trailer"));
    let trailer = t!(parse_with_lexer(lexer, resolve, ParseFlags::DICT));
    let trailer = t!(trailer.into_dictionary());

    Ok((sections, trailer))
}

pub fn read_xref_and_trailer_at(lexer: &mut Lexer, resolve: &impl Resolve) -> Result<(Vec<XRefSection>, Dictionary)> {
    let next_word = t!(lexer.next());
    if next_word == "xref" {
        // Read classic xref table
        parse_xref_table_and_trailer(lexer, resolve)
    } else {
        // Read xref stream
        lexer.back()?;
        parse_xref_stream_and_trailer(lexer, resolve)
    }
}
