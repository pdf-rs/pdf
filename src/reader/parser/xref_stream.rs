use reader::PdfReader;
use xref::*;
use err::*;
use num_traits::PrimInt;

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
}
