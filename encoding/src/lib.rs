#[macro_use] extern crate lazy_static;
use std::num::NonZeroU32;
use std::convert::TryInto;

#[derive(Eq, PartialEq, Hash, Copy, Clone, Debug)]
pub enum Encoding {
    Unicode,
    AdobeStandard,
    AdobeExpert,
    AdobeSymbol,
    AdobeZdingbat,
    WinAnsiEncoding,
}

pub enum Transcoder {
    Id,
    Forward(&'static ForwardMap), // X to to unicode
    Reverse(&'static ReverseMap), // unicode to X
    Both(&'static ForwardMap, &'static ReverseMap) // X to unicode to Y
}
impl Transcoder {
    pub fn translate(&self, codepoint: u32) -> Option<u32> {
        match self {
            Transcoder::Id => Some(codepoint),
            Transcoder::Forward(forward) => {
                codepoint.try_into().ok()
                .and_then(|b| forward.get(b))
                .map(|c| c as u32)
            }
            Transcoder::Reverse(reverse) => reverse.get(codepoint).map(|b| b as u32),
            Transcoder::Both(forward, reverse) => {
                codepoint.try_into().ok()
                .and_then(|b| forward.get(b))
                .and_then(|c| reverse.get(c as u32))
                .map(|b| b as u32)
            }
        }
    }
}

impl Encoding {
    pub fn forward_map(self) -> Option<&'static ForwardMap> {
        match self {
            Encoding::AdobeStandard => Some(&STANDARD),
            Encoding::AdobeSymbol => Some(&SYMBOL),
            Encoding::AdobeZdingbat => Some(&ZDINGBAT),
            Encoding::WinAnsiEncoding => Some(&WINANSI),
            _ => None
        }
    }
    pub fn reverse_map(self) -> Option<&'static ReverseMap> {
        match self {
            Encoding::AdobeStandard => Some(&UNICODE_TO_STANDARD),
            Encoding::AdobeSymbol => Some(&UNICODE_TO_SYMBOL),
            Encoding::AdobeZdingbat => Some(&UNICODE_TO_ZDINGBAT),
            Encoding::WinAnsiEncoding => Some(&UNICODE_TO_WINANSI),
            _ => None
        }
    }
    pub fn to(self, dest: Encoding) -> Option<Transcoder> {
        match (self, dest) {
            (source, dest) if source == dest => Some(Transcoder::Id),
            (source, Encoding::Unicode) => source.forward_map().map(|map| Transcoder::Forward(map)),
            (Encoding::Unicode, dest) => dest.reverse_map().map(|map| Transcoder::Reverse(map)),
            (source, dest) => source.forward_map()
                .and_then(|forward| 
                    dest.reverse_map().map(|reverse| Transcoder::Both(forward, reverse))
                )
        }
    }
}
pub struct ForwardMap([Option<Entry>; 256]);

impl ForwardMap {
    pub fn get(&self, codepoint: u8) -> Option<char> {
        self.0[codepoint as usize].map(|e| e.as_char())
    }
}
pub struct ReverseMap {
    chars: Vec<(u32, u8)>
}
impl ReverseMap {
    fn new(forward: &ForwardMap) -> ReverseMap {
        let mut chars: Vec<_> = forward.0.iter().enumerate()
            .filter_map(|(i, e)| e.map(|e| (e.as_u32(), i as u8)))
            .collect();
        chars.sort();
        ReverseMap { chars }
    }
    pub fn get(&self, c: u32) -> Option<u8> {
        self.chars.binary_search_by_key(&c, |&(c, _)| c).ok().map(|idx| self.chars[idx].1)
    }
}

lazy_static! {
    static ref UNICODE_TO_STANDARD: ReverseMap = ReverseMap::new(&STANDARD);
    static ref UNICODE_TO_SYMBOL: ReverseMap = ReverseMap::new(&SYMBOL);
    static ref UNICODE_TO_ZDINGBAT: ReverseMap = ReverseMap::new(&ZDINGBAT);
    static ref UNICODE_TO_WINANSI: ReverseMap = ReverseMap::new(&WINANSI);
}

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
    pub fn as_u32(&self) -> u32 {
        self.0.get()
    }
}
        
// we rely on the encoding not producing '\0'.
const fn c(c: char) -> Option<Entry> {
    Some(Entry::new(c))
}

pub static STANDARD: ForwardMap = ForwardMap(include!("stdenc.rs"));
pub static SYMBOL: ForwardMap = ForwardMap(include!("symbol.rs"));
pub static ZDINGBAT: ForwardMap = ForwardMap(include!("zdingbat.rs"));
pub static WINANSI: ForwardMap = ForwardMap(include!("cp1252.rs"));

#[test]
fn test_forward() {
    assert_eq!(STANDARD.get(0xD0), Some('\u{2014}'));
}
#[test]
fn test_reverse() {
    assert_eq!(UNICODE_TO_STANDARD.get(0x2014), Some(0xD0));
}
