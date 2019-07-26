use std::num::NonZeroU32;
use std::io;
use std::collections::HashMap;
use crate::object::{Object, Resolve};
use crate::primitive::Primitive;
use crate::error::{Result, PdfError};

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

#[derive(Debug, Clone)]
pub struct Encoding {
    pub base: BaseEncoding,
    pub differences: HashMap<u32, String>,
}

#[derive(Object, Debug, Clone)]
pub enum BaseEncoding {
    StandardEncoding,
    SymbolEncoding,
    MacRomanEncoding,
    WinAnsiEncoding,
    MacExpertEncoding,
    #[pdf(name="Identity-H")]
    IdentityH,
    None
}
impl Object for Encoding {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {unimplemented!()}
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            name @ Primitive::Name(_) => { 
                Ok(Encoding {
                    base: BaseEncoding::from_primitive(name, resolve)?,
                    differences: HashMap::new(),
                })
            }
            Primitive::Dictionary(mut dict) => {
                let base = match dict.remove("BaseEncoding") {
                    Some(p) => BaseEncoding::from_primitive(p, resolve)?,
                    None => BaseEncoding::None
                };
                let mut gid = 0;
                let mut differences = HashMap::new();
                if let Some(p) = dict.remove("Differences") {
                    for part in p.to_array(resolve)? {
                        match part {
                            Primitive::Integer(code) => {
                                gid = code as u32;
                            }
                            Primitive::Name(name) => {
                                differences.insert(gid, name);
                                gid += 1;
                            },
                            _ => panic!()
                        }
                    }
                }
                Ok(Encoding { base, differences })
            }
            Primitive::Reference(r) => Self::from_primitive(resolve.resolve(r)?, resolve),
            _ => panic!()
        }
    }
}
impl Encoding { 
    pub fn standard() -> Encoding {
        Encoding {
            base: BaseEncoding::StandardEncoding,
            differences: HashMap::new()
        }
    }
}

impl BaseEncoding {
    fn map(&self) -> Option<&[Option<Entry>; 256]> {
        match self {
            BaseEncoding::SymbolEncoding => Some(&SYMBOL),
            BaseEncoding::StandardEncoding => Some(&STANDARD),
            _ => None
        }
    }
    pub fn decode_byte(&self, b: u8) -> Option<char> {
        match self.map() {
            Some(map) => map[b as usize].map(|e| e.as_char()),
            None => Some(b as char)
        }
    }
    pub fn decode_bytes(&self, data: &[u8]) -> String {
        match self.map() {
            Some(map) => data.iter().flat_map(|&b| map[b as usize].map(|e| e.as_char())).collect(),
            None => data.iter().map(|&b| b as char).collect()
        }
    }
}
