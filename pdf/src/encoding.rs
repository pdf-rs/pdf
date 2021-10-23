use crate as pdf;
use crate::error::Result;
use crate::object::{Object, Resolve};
use crate::primitive::Primitive;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Encoding {
    pub base:        BaseEncoding,
    pub differences: HashMap<u32, String>,
}

#[derive(Object, Debug, Clone, Eq, PartialEq)]
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
            name @ Primitive::Name(_) => Ok(Encoding {
                base:        BaseEncoding::from_primitive(name, resolve)?,
                differences: HashMap::new(),
            }),
            Primitive::Dictionary(mut dict) => {
                let base = match dict.remove("BaseEncoding") {
                    Some(p) => BaseEncoding::from_primitive(p, resolve)?,
                    None => BaseEncoding::None,
                };
                let mut gid = 0;
                let mut differences = HashMap::new();
                if let Some(p) = dict.remove("Differences") {
                    for part in p.into_array(resolve)? {
                        match part {
                            Primitive::Integer(code) => {
                                gid = code as u32;
                            }
                            Primitive::Name(name) => {
                                differences.insert(gid, name);
                                gid += 1;
                            }
                            _ => panic!(),
                        }
                    }
                }
                Ok(Encoding { base, differences })
            }
            Primitive::Reference(r) => Self::from_primitive(resolve.resolve(r)?, resolve),
            _ => panic!(),
        }
    }
}
impl Encoding {
    pub fn standard() -> Encoding {
        Encoding {
            base:        BaseEncoding::StandardEncoding,
            differences: HashMap::new(),
        }
    }
}
