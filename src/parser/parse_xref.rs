use err::*;
use num_traits::PrimInt;
use parser::lexer::Lexer;
use xref::{XRef, XRefSection};
use file::{XRefStream};
use primitive::*;
use object::*;
use parser::{parse, parse_with_lexer};
use parser::parse_object::{parse_indirect_object, parse_indirect_stream};


// Just the part of Parser which reads xref sections from xref stream.
/// Takes `&mut &[u8]` so that it can "consume" data as it reads
pub fn parse_xref_section_from_stream(first_id: i32, num_entries: i32, width: &[i32], data: &mut &[u8]) -> Result<XRefSection> {
    let mut entries = Vec::new();
    for _ in 0..num_entries {
        let _type = read_u64_from_stream(width[0], data);
        let field1 = read_u64_from_stream(width[1], data);
        let field2 = read_u64_from_stream(width[2], data);

        let entry =
        match _type {
            0 => XRef::Free {next_obj_nr: field1 as ObjNr, gen_nr: field2 as GenNr},
            1 => XRef::Raw {pos: field1 as usize, gen_nr: field2 as GenNr},
            2 => XRef::Stream {stream_id: field1 as ObjNr, index: field2 as usize},
            _ => bail!("Reading xref stream, The first field 'type' is {} - must be 0, 1 or 2", _type),
        };
        entries.push(entry);
    }
    Ok(XRefSection {
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


/// Reads xref sections (from stream) and trailer starting at the position of the Lexer.
pub fn parse_xref_stream_and_trailer<'a>(lexer: &mut Lexer) -> Result<Vec<XRefSection>> {
    let xref_stream = parse_indirect_stream(lexer).chain_err(|| "Reading Xref stream")?.1;
    let xref_stream = XRefStream::from_stream(&xref_stream, no_resolve)?;


    // Get 'W' as array of integers
    let width = &xref_stream.info.w;
    let num_entries = &xref_stream.info.size;
    let indices = &xref_stream.info.index;
    
    let mut data_left = &xref_stream.data[..];

    let mut sections = Vec::new();
    for (first_id, num_objects) in indices.chunks(2).map(|c| (c[0], c[1])) {
        let section = parse_xref_section_from_stream(first_id, num_objects, &width, &mut data_left)?;
        sections.push(section);
    }
    // debug!("Xref stream"; "Sections" => format!("{:?}", sections));

    Ok(sections)
}


/// Reads xref sections (from table) and trailer starting at the position of the Lexer.
pub fn parse_xref_table_and_trailer(lexer: &mut Lexer) -> Result<(Vec<XRefSection>, Dictionary)> {
    let mut sections = Vec::new();
    
    // Keep reading subsections until we hit `trailer`
    while !lexer.peek()?.equals(b"trailer") {
        let start_id = lexer.next_as::<u32>()?;
        let num_ids = lexer.next_as::<u32>()?;

        let mut section = XRefSection::new(start_id);

        for _ in 0..num_ids {
            let w1 = lexer.next()?;
            let w2 = lexer.next()?;
            let w3 = lexer.next()?;
            if w3.equals(b"f") {
                section.add_free_entry(w1.to::<ObjNr>()?, w2.to::<GenNr>()?);
            } else if w3.equals(b"n") {
                section.add_inuse_entry(w1.to::<usize>()?, w2.to::<GenNr>()?);
            } else {
                bail!(ErrorKind::UnexpectedLexeme {pos: lexer.get_pos(), lexeme: w3.to_string(), expected: "f or n"});
            }
        }
        sections.push(section);
    }
    // Read trailer
    lexer.next_expect("trailer")?;
    let trailer = parse_with_lexer(lexer)?.as_dictionary(no_resolve)?.clone(); // TODO clones dictionary. Better solution?
 
    Ok((sections, trailer))
}

