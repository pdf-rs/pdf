use datasize::DataSize;
use crate as pdf;
use crate::object::*;
use crate::error::*;

#[derive(Object, Debug, DataSize, DeepClone, ObjectWrite)]
pub struct IccInfo {
    #[pdf(key="N")]
    pub components: u32,

    #[pdf(key="Alternate")]
    pub alternate: Option<Box<ColorSpace>>,

    #[pdf(key="Range")]
    pub range: Option<Vec<f32>>,

    #[pdf(key="Metadata")]
    pub metadata: Option<Stream<()>>,
}

/// Parameters of a CIE 1976 L*a*b* colour space (PDF 32000-1 §8.6.5.4).
#[derive(Object, Debug, Clone, DataSize, DeepClone, ObjectWrite)]
pub struct LabParams {
    /// `[Xw Yw Zw]` — the diffuse white point (required).
    #[pdf(key = "WhitePoint")]
    pub white_point: Vec<f32>,

    /// `[Xb Yb Zb]` — the diffuse black point. Defaults to `[0 0 0]`.
    #[pdf(key = "BlackPoint")]
    pub black_point: Option<Vec<f32>>,

    /// `[amin amax bmin bmax]` — the range of the a* and b* components.
    /// Defaults to `[-100 100 -100 100]`.
    #[pdf(key = "Range")]
    pub range: Option<Vec<f32>>,
}

#[derive(Debug, Clone, DeepClone)]
pub enum ColorSpace {
    DeviceGray,
    DeviceRGB,
    DeviceCMYK,
    DeviceN { names: Vec<Name>, alt: Box<ColorSpace>, tint: Function, attr: Option<Dictionary> },
    CalGray(Dictionary),
    CalRGB(Dictionary),
    CalCMYK(Dictionary),
    Lab(LabParams),
    Indexed(Box<ColorSpace>, u8, Arc<[u8]>),
    Separation(Name, Box<ColorSpace>, Function),
    Icc(RcRef<Stream<IccInfo>>),
    Pattern,
    Named(Name),
    Other(Vec<Primitive>)
}
impl DataSize for ColorSpace {
    const IS_DYNAMIC: bool = true;
    const STATIC_HEAP_SIZE: usize = 0;

    #[inline]
    fn estimate_heap_size(&self) -> usize {
        match *self {
            ColorSpace::DeviceGray | ColorSpace::DeviceRGB | ColorSpace::DeviceCMYK => 0,
            ColorSpace::DeviceN { ref names, ref alt, ref tint, ref attr } => {
                names.estimate_heap_size() +
                alt.estimate_heap_size() +
                tint.estimate_heap_size() +
                attr.estimate_heap_size()
            }
            ColorSpace::CalGray(ref d) | ColorSpace::CalRGB(ref d) | ColorSpace::CalCMYK(ref d) => {
                d.estimate_heap_size()
            }
            ColorSpace::Lab(ref p) => p.estimate_heap_size(),
            ColorSpace::Indexed(ref cs, _, ref data) => {
                cs.estimate_heap_size() + data.estimate_heap_size()
            }
            ColorSpace::Separation(ref name, ref cs, ref f) => {
                name.estimate_heap_size() + cs.estimate_heap_size() + f.estimate_heap_size()
            }
            ColorSpace::Icc(ref s) => s.estimate_heap_size(),
            ColorSpace::Pattern => 0,
            ColorSpace::Other(ref v) => v.estimate_heap_size(),
            ColorSpace::Named(ref n) => n.estimate_heap_size()
        }
    }
}

fn get_index(arr: &[Primitive], idx: usize) -> Result<&Primitive> {
     arr.get(idx).ok_or(PdfError::Bounds { index: idx, len: arr.len() })
}

impl Object for ColorSpace {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<ColorSpace> {
        ColorSpace::from_primitive_depth(p, resolve, 5)
    }
}
impl ColorSpace {
    fn from_primitive_depth(p: Primitive, resolve: &impl Resolve, depth: usize) -> Result<ColorSpace> {
        let p = p.resolve(resolve)?;

        if let Ok(name) = p.as_name() {
            let cs = match name {
                "DeviceGray" => ColorSpace::DeviceGray,
                "DeviceRGB" => ColorSpace::DeviceRGB,
                "DeviceCMYK" => ColorSpace::DeviceCMYK,
                "Pattern" => ColorSpace::Pattern,
                name => ColorSpace::Named(name.into()),
            };
            return Ok(cs);
        }
        let arr = t!(p.into_array());
        let typ_p = t!(get_index(&arr, 0)).clone().resolve(resolve)?;
        let typ = t!(typ_p.as_name());

        if depth == 0 {
            bail!("ColorSpace base recursion");
        }
        match typ {
            "Indexed" => {
                let base = Box::new(t!(ColorSpace::from_primitive_depth(t!(get_index(&arr, 1)).clone(), resolve, depth-1)));
                let hival = t!(t!(get_index(&arr, 2)).as_u8());
                let lookup = match t!(get_index(&arr, 3)) {
                    &Primitive::Reference(r) => resolve.resolve(r)?,
                    p => p.clone()
                };
                let lookup = match lookup {
                    Primitive::String(string) => {
                        let data: Vec<u8> = string.into_bytes().into();
                        data.into()
                    }
                    Primitive::Stream(stream) => {
                        let s: Stream::<()> = Stream::from_stream(stream, resolve)?;
                        t!(s.data(resolve))
                    },
                    p => return Err(PdfError::UnexpectedPrimitive {
                        expected: "String or Stream",
                        found: p.get_debug_name()
                    })
                };
                Ok(ColorSpace::Indexed(base, hival, lookup))
            }
            "Separation" => {
                let name = t!(t!(get_index(&arr, 1)).clone().into_name());
                let alternate = Box::new(t!(ColorSpace::from_primitive_depth(t!(get_index(&arr, 2)).clone(), resolve, depth-1)));
                let tint = t!(Function::from_primitive(t!(get_index(&arr, 3)).clone(), resolve));
                Ok(ColorSpace::Separation(name, alternate, tint))
            }
            "ICCBased" => {
                let s = t!(RcRef::from_primitive(t!(get_index(&arr, 1)).clone(), resolve));
                Ok(ColorSpace::Icc(s))
            }
            "DeviceN" => {
                let names = t!(Object::from_primitive(t!(get_index(&arr, 1)).clone(), resolve));
                let alt = t!(Object::from_primitive(t!(get_index(&arr, 2)).clone(), resolve));
                let tint = t!(Function::from_primitive(t!(get_index(&arr, 3)).clone(), resolve));
                let attr = arr.get(4).map(|p| Dictionary::from_primitive(p.clone(), resolve)).transpose()?;

                Ok(ColorSpace::DeviceN { names, alt, tint, attr})
            }
            "CalGray" => {
                let dict = Dictionary::from_primitive(t!(get_index(&arr, 1)).clone(), resolve)?;
                Ok(ColorSpace::CalGray(dict))
            }
            "CalRGB" => {
                let dict = Dictionary::from_primitive(t!(get_index(&arr, 1)).clone(), resolve)?;
                Ok(ColorSpace::CalRGB(dict))
            }
            "CalCMYK" => {
                let dict = Dictionary::from_primitive(t!(get_index(&arr, 1)).clone(), resolve)?;
                Ok(ColorSpace::CalCMYK(dict))
            }
            "Lab" => {
                let params = LabParams::from_primitive(t!(get_index(&arr, 1)).clone(), resolve)?;
                Ok(ColorSpace::Lab(params))
            }
            "Pattern" => {
                Ok(ColorSpace::Pattern)
            }
            _ => Ok(ColorSpace::Other(arr))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::NoResolve;

    // `[/Lab << /WhitePoint [..] /Range [..] >>]` should parse into the typed
    // `Lab` variant rather than falling through to `Other`.
    #[test]
    fn lab_colorspace_parses() {
        let mut dict = Dictionary::new();
        dict.insert(
            "WhitePoint",
            Primitive::Array(vec![Primitive::Number(0.9505), Primitive::Number(1.0), Primitive::Number(1.089)]),
        );
        dict.insert(
            "Range",
            Primitive::Array(vec![
                Primitive::Number(-128.0),
                Primitive::Number(127.0),
                Primitive::Number(-128.0),
                Primitive::Number(127.0),
            ]),
        );
        let arr = Primitive::Array(vec![Primitive::name("Lab"), Primitive::Dictionary(dict)]);

        match ColorSpace::from_primitive(arr, &NoResolve).unwrap() {
            ColorSpace::Lab(params) => {
                assert_eq!(params.white_point, vec![0.9505, 1.0, 1.089]);
                assert_eq!(params.black_point, None);
                assert_eq!(params.range.as_deref(), Some([-128.0, 127.0, -128.0, 127.0].as_slice()));
            }
            other => panic!("expected ColorSpace::Lab, got {other:?}"),
        }
    }
}

impl ObjectWrite for ColorSpace {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match *self {
            ColorSpace::DeviceCMYK => Ok(Primitive::name("DeviceCMYK")),
            ColorSpace::DeviceRGB => Ok(Primitive::name("DeviceRGB")),
            ColorSpace::DeviceGray => Ok(Primitive::name("DeviceGray")),
            ColorSpace::Indexed(ref  base, hival, ref lookup) => {
                let base = base.to_primitive(update)?;
                let hival = Primitive::Integer(hival.into());
                let lookup = if lookup.len() < 100 {
                    PdfString::new((**lookup).into()).into()
                } else {
                    Stream::new((), &lookup)?.to_primitive(update)?
                };
                Ok(Primitive::Array(vec![Primitive::name("Indexed"), base, hival, lookup]))
            }
            ColorSpace::Lab(ref params) => {
                Ok(Primitive::Array(vec![Primitive::name("Lab"), params.to_primitive(update)?]))
            }
            ref p => {
                dbg!(p);
                unimplemented!()
            }
        }
    }
}
