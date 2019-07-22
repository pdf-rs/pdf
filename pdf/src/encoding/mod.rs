use std::num::NonZeroU32;

#[derive(Copy, Clone)]
struct Entry(NonZeroU32);
impl Entry {
    const fn new(c: char) -> Entry {
        Entry(
            unsafe {
                NonZeroU32::new_unchecked(c as u32)
            }
        )
    }
    fn as_char(&self) -> char {
        std::char::from_u32(self.0.get()).unwrap()
    }
}
        
// we rely on the encoding not producing '\0'.
const fn c(c: char) -> Option<Entry> {
    Some(Entry::new(c))
}
static STANDARD: [Option<Entry>; 256] = include!("stdenc.rs");
static SYMBOL: [Option<Entry>; 256] = include!("symbol.rs");
static ZDINGBAT: [Option<Entry>; 256] = include!("zdingbat.rs");

#[derive(Object, Debug, Clone)]
pub enum Encoding {
    StandardEncoding,
    SymbolEncoding,
    MacRomanEncoding,
    WinAnsiEncoding,
    MacExpertEncoding,
    None
}

#[derive(Clone)]
pub struct Decoder {
    map: Option<&'static [Option<Entry>; 256]>
}
impl Decoder {
    pub fn new(encoding: &Encoding) -> Decoder {
        let map = match encoding {
            Encoding::SymbolEncoding => Some(&SYMBOL),
            Encoding::StandardEncoding => Some(&STANDARD),
            _ => None
        };
        Decoder { map }
    }
    pub fn decode_byte(&self, b: u8) -> Option<char> {
        match self.map {
            Some(map) => map[b as usize].map(|e| e.as_char()),
            None => Some(b as char)
        }
    }
    pub fn decode_bytes(&self, data: &[u8]) -> String {
        match self.map {
            Some(map) => data.iter().flat_map(|&b| map[b as usize].map(|e| e.as_char())).collect(),
            None => data.iter().map(|&b| b as char).collect()
        }
    }
}
