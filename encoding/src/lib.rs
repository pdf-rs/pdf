use std::num::NonZeroU32;

pub enum Encoding {
    Unicode,
    AdobeStandard,
    AdobeSymbol,
    AdobeZdingbat,
    WinAnsiEncoding,
}
/*
pub fn build_map(source: Encoding, dest: Encoding) -> HashMap<u32, u32> {

}
*/

#[derive(Copy, Clone)]
pub struct Entry(NonZeroU32);
impl Entry {
    const fn new(c: char) -> Entry {
        Entry(
            unsafe {
                NonZeroU32::new_unchecked(c as u32)
            }
        )
    }
    pub fn as_char(&self) -> char {
        std::char::from_u32(self.0.get()).unwrap()
    }
}
        
// we rely on the encoding not producing '\0'.
const fn c(c: char) -> Option<Entry> {
    Some(Entry::new(c))
}

pub static STANDARD: [Option<Entry>; 256] = include!("stdenc.rs");
pub static SYMBOL: [Option<Entry>; 256] = include!("symbol.rs");
pub static ZDINGBAT: [Option<Entry>; 256] = include!("zdingbat.rs");
pub static WINANSI: [Option<Entry>; 256] = include!("cp1252.rs");
