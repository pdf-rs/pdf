use super::prelude::*;

#[derive(Debug, Clone, DataSize)]
pub enum MaybeNamedDest {
    Named(PdfString),
    Direct(Dest),
}

#[derive(Debug, Clone, DataSize)]
pub struct Dest {
    pub page: Option<Ref<Page>>,
    pub view: DestView,
}
impl Object for Dest {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let p = match p {
            Primitive::Reference(r) => resolve.resolve(r)?,
            p => p,
        };
        let p = match p {
            Primitive::Dictionary(mut dict) => dict.require("Dest", "D")?,
            p => p,
        };
        let array = t!(p.as_array(), p);
        Dest::from_array(array, resolve)
    }
}
impl Dest {
    fn from_array(array: &[Primitive], resolve: &impl Resolve) -> Result<Self> {
        let page = Object::from_primitive(try_opt!(array.get(0)).clone(), resolve)?;
        let kind = try_opt!(array.get(1));
        let view = match kind.as_name()? {
            "XYZ" => DestView::XYZ {
                left: match *try_opt!(array.get(2)) {
                    Primitive::Null => None,
                    Primitive::Integer(n) => Some(n as f32),
                    Primitive::Number(f) => Some(f),
                    ref p => {
                        return Err(PdfError::UnexpectedPrimitive {
                            expected: "Number | Integer | Null",
                            found: p.get_debug_name(),
                        })
                    }
                },
                top: match *try_opt!(array.get(3)) {
                    Primitive::Null => None,
                    Primitive::Integer(n) => Some(n as f32),
                    Primitive::Number(f) => Some(f),
                    ref p => {
                        return Err(PdfError::UnexpectedPrimitive {
                            expected: "Number | Integer | Null",
                            found: p.get_debug_name(),
                        })
                    }
                },
                zoom: match array.get(4) {
                    Some(Primitive::Null) => 0.0,
                    Some(&Primitive::Integer(n)) => n as f32,
                    Some(&Primitive::Number(f)) => f,
                    Some(p) => {
                        return Err(PdfError::UnexpectedPrimitive {
                            expected: "Number | Integer | Null",
                            found: p.get_debug_name(),
                        })
                    }
                    None => 0.0,
                },
            },
            "Fit" => DestView::Fit,
            "FitH" => DestView::FitH {
                top: try_opt!(array.get(2)).as_number()?,
            },
            "FitV" => DestView::FitV {
                left: try_opt!(array.get(2)).as_number()?,
            },
            "FitR" => DestView::FitR(Rectangle {
                left: try_opt!(array.get(2)).as_number()?,
                bottom: try_opt!(array.get(3)).as_number()?,
                right: try_opt!(array.get(4)).as_number()?,
                top: try_opt!(array.get(5)).as_number()?,
            }),
            "FitB" => DestView::FitB,
            "FitBH" => DestView::FitBH {
                top: try_opt!(array.get(2)).as_number()?,
            },
            name => {
                return Err(PdfError::UnknownVariant {
                    id: "Dest",
                    name: name.into(),
                })
            }
        };
        Ok(Dest { page, view })
    }
}
impl Object for MaybeNamedDest {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let p = match p {
            Primitive::Reference(r) => resolve.resolve(r)?,
            p => p,
        };
        let p = match p {
            Primitive::Dictionary(mut dict) => dict.require("Dest", "D")?,
            Primitive::String(s) => return Ok(MaybeNamedDest::Named(s)),
            p => p,
        };
        let array = t!(p.as_array(), p);
        Dest::from_array(array, resolve).map(MaybeNamedDest::Direct)
    }
}
impl ObjectWrite for MaybeNamedDest {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            MaybeNamedDest::Named(s) => Ok(Primitive::String(s.clone())),
            MaybeNamedDest::Direct(d) => d.to_primitive(update),
        }
    }
}
impl ObjectWrite for Dest {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        let mut arr = vec![self.page.to_primitive(update)?];
        match self.view {
            DestView::XYZ { left, top, zoom } => {
                arr.push(Primitive::Name("XYZ".into()));
                arr.push(left.to_primitive(update)?);
                arr.push(top.to_primitive(update)?);
                arr.push(Primitive::Number(zoom));
            }
            DestView::Fit => {
                arr.push(Primitive::Name("Fit".into()));
            }
            DestView::FitH { top } => {
                arr.push(Primitive::Name("FitH".into()));
                arr.push(Primitive::Number(top));
            }
            DestView::FitV { left } => {
                arr.push(Primitive::Name("FitV".into()));
                arr.push(Primitive::Number(left));
            }
            DestView::FitR(rect) => {
                arr.push(Primitive::Name("FitR".into()));
                arr.push(Primitive::Number(rect.left));
                arr.push(Primitive::Number(rect.bottom));
                arr.push(Primitive::Number(rect.right));
                arr.push(Primitive::Number(rect.top));
            }
            DestView::FitB => {
                arr.push(Primitive::Name("FitB".into()));
            }
            DestView::FitBH { top } => {
                arr.push(Primitive::Name("FitBH".into()));
                arr.push(Primitive::Number(top));
            }
        }
        Ok(Primitive::Array(arr))
    }
}
