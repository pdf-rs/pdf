use crate as pdf;
use crate::error::Result;
use crate::object::{DeepClone, Object, ObjectWrite, Resolve};
use crate::primitive::{Dictionary, Primitive};
use datasize::DataSize;
use istring::SmallString;
use std::collections::HashMap;

#[derive(Debug, Clone, DataSize)]
pub struct Encoding {
    pub base: BaseEncoding,
    pub differences: HashMap<u32, SmallString>,
}

#[derive(Object, ObjectWrite, Debug, Clone, Eq, PartialEq, DataSize)]
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
                base: BaseEncoding::from_primitive(name, resolve)?,
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
                    for part in p.resolve(resolve)?.into_array()? {
                        match part {
                            Primitive::Integer(code) => {
                                gid = code as u32;
                            }
                            Primitive::Name(name) => {
                                differences.insert(gid, name);
                                gid += 1;
                            }
                            _ => bail!("Unknown part primitive in dictionary: {:?}", part),
                        }
                    }
                }
                Ok(Encoding { base, differences })
            }
            Primitive::Reference(r) => Self::from_primitive(resolve.resolve(r)?, resolve),
            Primitive::Stream(s) => Self::from_primitive(Primitive::Dictionary(s.info), resolve),
            _ => bail!("Unknown element: {:?}", p),
        }
    }
}
impl ObjectWrite for Encoding {
    fn to_primitive(&self, update: &mut impl pdf::object::Updater) -> Result<Primitive> {
        let base = self.base.to_primitive(update)?;
        if self.differences.len() == 0 {
            Ok(base)
        } else {
            let mut list = vec![];

            let mut diff_list: Vec<_> = self.differences.iter().collect();
            diff_list.sort();
            let mut last = None;

            for &(&gid, name) in diff_list.iter() {
                if !last.map(|n| n + 1 == gid).unwrap_or(false) {
                    list.push(Primitive::Integer(gid as i32));
                }

                list.push(Primitive::Name(name.clone()));

                last = Some(gid);
            }

            let mut dict = Dictionary::new();
            dict.insert("BaseEncoding", base);
            dict.insert("Differences", Primitive::Array(list));
            Ok(Primitive::Dictionary(dict))
        }
    }
}
impl Encoding {
    pub fn standard() -> Encoding {
        Encoding {
            base: BaseEncoding::StandardEncoding,
            differences: HashMap::new(),
        }
    }
}
impl DeepClone for Encoding {
    fn deep_clone(&self, _cloner: &mut impl pdf::object::Cloner) -> Result<Self> {
        Ok(self.clone())
    }
}
