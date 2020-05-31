use crate as pdf;
use crate::object::*;
use crate::error::*;

#[derive(Object, Debug)]
pub struct IccInfo {
    #[pdf(key="N")]
    pub components: u32,

    #[pdf(key="Alternate")]
    pub alternate: Option<Rc<ColorSpace>>,

    #[pdf(key="Range")]
    pub range: Option<Vec<f32>>,

    #[pdf(key="Metadata")]
    pub metadata: Option<Stream<()>>,
}

#[derive(Debug)]
pub enum ColorSpace {
    Separation(String, Rc<ColorSpace>, Function),
    Icc(Stream<IccInfo>),
    Other(Vec<Primitive>)
}


fn get_index(arr: &[Primitive], idx: usize) -> Result<&Primitive> {
     arr.get(idx).ok_or(PdfError::Bounds { index: idx, len: arr.len() })
}

impl Object for ColorSpace {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        unimplemented!()
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<ColorSpace> {
        dbg!(&p);
        let arr = t!(p.to_array(resolve));
        dbg!(&arr);
        let typ = t!(t!(get_index(&arr, 0)).as_name());
        
        match typ {
            "Separation" => {
                let name = t!(t!(get_index(&arr, 1)).clone().to_name());
                let alternate = t!(Object::from_primitive(t!(get_index(&arr, 2)).clone(), resolve));
                let tint = t!(Function::from_primitive(t!(get_index(&arr, 3)).clone(), resolve));
                Ok(ColorSpace::Separation(name, alternate, tint))
            }
            "ICCBased" => {
                let s: Stream<IccInfo> = t!(Stream::from_primitive(t!(get_index(&arr, 1)).clone(), resolve));
                Ok(ColorSpace::Icc(s))
            }
            _ => Ok(ColorSpace::Other(arr))
        }
    }
}
