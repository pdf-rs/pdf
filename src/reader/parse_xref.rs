use reader::PdfReader;
use xref::*;
use err::*;
use object::*;
use num_traits::PrimInt;
use reader::lexer::Lexer;

// Just the part of Parser which reads xref sections from xref stream.
impl PdfReader {
    /// Takes `&mut &[u8]` so that it can "consume" data as it reads
    pub fn parse_xref_section_from_stream(first_id: i32, num_entries: i32, width: &Vec<i32>, data: &mut &[u8]) -> Result<XrefSection> {
        let mut entries = Vec::new();
        for _ in 0..num_entries {
            let _type = PdfReader::read_u64_from_stream(width[0], data);
            let field1 = PdfReader::read_u64_from_stream(width[1], data);
            let field2 = PdfReader::read_u64_from_stream(width[2], data);

            let entry =
            match _type {
                0 => XrefEntry::Free {next_obj_nr: field1 as u32, gen_nr: field2 as u16},
                1 => XrefEntry::InUse {pos: field1 as usize, gen_nr: field2 as u16},
                2 => XrefEntry::InStream {stream_obj_nr: field1 as u32, index: field2 as u16},
                _ => bail!("Reading xref stream, The first field 'type' is {} - must be 0, 1 or 2", _type),
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


    /// Reads xref sections (from stream) and trailer starting at the position of the Lexer.
    pub fn parse_xref_stream_and_trailer(&self, lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
        let xref_stream = self.parse_indirect_object(lexer).chain_err(|| "Reading Xref stream")?.object.as_stream()?;

        // Get 'W' as array of integers
        let width = xref_stream.dictionary.get("W")?.borrow_integer_array()?;
        let num_entries = xref_stream.dictionary.get("Size")?.as_integer()?;

        let indices: Vec<(i32, i32)> = {
            match xref_stream.dictionary.get("Index") {
                Ok(obj) => obj.borrow_integer_array()?,
                Err(_) => vec![0, num_entries],
            }.chunks(2).map(|c| (c[0], c[1])).collect()
            // ^^ TODO panics if odd number of elements - how to handle it?
        };
        
        let (dict, data) = (xref_stream.dictionary, xref_stream.content);
        
        let mut data_left = &data[..];

        let mut sections = Vec::new();
        for (first_id, num_objects) in indices {
            let section = PdfReader::parse_xref_section_from_stream(first_id, num_objects, &width, &mut data_left)?;
            sections.push(section);
        }
        // debug!("Xref stream"; "Sections" => format!("{:?}", sections));

        Ok((sections, dict))
    }


    /// Reads xref sections (from table) and trailer starting at the position of the Lexer.
    pub fn parse_xref_table_and_trailer(&self, lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
        let mut sections = Vec::new();
        
        // Keep reading subsections until we hit `trailer`
        while !lexer.peek()?.equals(b"trailer") {
            let start_id = lexer.next_as::<u32>()?;
            let num_ids = lexer.next_as::<u32>()?;

            let mut section = XrefSection::new(start_id);

            for _ in 0..num_ids {
                let w1 = lexer.next()?;
                let w2 = lexer.next()?;
                let w3 = lexer.next()?;
                if w3.equals(b"f") {
                    section.add_free_entry(w1.to::<u32>()?, w2.to::<u16>()?);
                } else if w3.equals(b"n") {
                    section.add_inuse_entry(w1.to::<usize>()?, w2.to::<u16>()?);
                } else {
                    bail!(ErrorKind::UnexpectedLexeme {pos: lexer.get_pos(), lexeme: w3.as_string(), expected: "f or n"});
                }
            }
            sections.push(section);
        }
        // Read trailer
        lexer.next_expect("trailer")?;
        let trailer = self.parse_object(lexer)?.as_dictionary()?;
     
        Ok((sections, trailer))
    }

}
