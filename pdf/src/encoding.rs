use std::collections::HashMap;
use istring::SmallString;
use crate as pdf;
use crate::object::{Object, Resolve};
use crate::primitive::Primitive;
use crate::error::{Result};
use datasize::DataSize;

#[derive(Debug, Clone, DataSize)]
pub struct Encoding {
    pub base: BaseEncoding,
    pub differences: HashMap<u32, SmallString>,
}

#[derive(Object, Debug, Clone, Eq, PartialEq, DataSize)]
pub enum BaseEncoding {
    StandardEncoding,
    SymbolEncoding,
    MacRomanEncoding,
    WinAnsiEncoding,
    MacExpertEncoding,
    #[pdf(name = "Identity-H")]
    IdentityH,
    None,

    #[pdf(other)]
    Other(String),
}
impl Object for Encoding {
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
                    for part in p.resolve(resolve)?.into_array()? {
                        match part {
                            Primitive::Integer(code) => {
                                gid = code as u32;
                            }
                            Primitive::Name(name) => {
                                differences.insert(gid, name);
                                gid += 1;
                            }
                            _ => panic!("Unknown part primitive in dictionary: {:?}", part),
                        }
                    }
                }
                Ok(Encoding { base, differences })
            }
            Primitive::Reference(r) => Self::from_primitive(resolve.resolve(r)?, resolve),
            Primitive::Stream(s) => Self::from_primitive(Primitive::Dictionary(s.info), resolve),
            _ => panic!("Unknown element: {:?}", p),
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
