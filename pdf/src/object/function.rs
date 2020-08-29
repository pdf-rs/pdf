use crate as pdf;
use crate::object::*;
use crate::error::*;

#[derive(Object, Debug)]
struct RawFunction {
    #[pdf(key="FunctionType")]
    function_type: u32,

    #[pdf(key="Domain")]
    domain: Vec<f32>,

    #[pdf(key="Range")]
    range: Option<Vec<f32>>,

    #[pdf(other)]
    other: Dictionary
}

#[derive(Object, Debug)]
struct Function2 {
    #[pdf(key="C0")]
    c0: Option<Vec<f32>>,

    #[pdf(key="C1")]
    c1: Option<Vec<f32>>,

    #[pdf(key="N")]
    exponent: f32,
}

#[derive(Debug)]
pub enum Function {
    Sampled,
    Interpolated(Vec<InterpolatedFunctionDim>),
    Stiching,
    Calculator,
}
impl Function {
    pub fn apply(&self, x: f32, out: &mut [f32]) {
        match *self {
            Function::Interpolated(ref parts) => {
                for (f, y) in parts.iter().zip(out) {
                    *y = f.apply(x);
                }
            }
            _ => panic!("unimplemted function {:?}", self)
        }
    }
}
impl Object for Function {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {
        unimplemented!()
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        use std::f32::INFINITY;
        let raw = RawFunction::from_primitive(p, resolve)?;
        match raw.function_type {
            2 => {
                let f2 = Function2::from_dict(raw.other, resolve)?;
                let mut parts = Vec::with_capacity(raw.domain.len());
                
                let n_dim = match (raw.range.as_ref(), f2.c0.as_ref(), f2.c1.as_ref()) {
                    (Some(range), _, _) => range.len() / 2,
                    (_, Some(c0), _) => c0.len(),
                    (_, _, Some(c1)) => c1.len(),
                    _ => panic!("unknown dimensions")
                };
                let input_range = (raw.domain[0], raw.domain[1]);
                for dim in 0 .. n_dim {
                    let output_range = (
                        raw.range.as_ref().and_then(|r| r.get(2*dim).cloned()).unwrap_or(-INFINITY),
                        raw.range.as_ref().and_then(|r| r.get(2*dim+1).cloned()).unwrap_or(INFINITY)
                    );
                    let c0 = f2.c0.as_ref().and_then(|c0| c0.get(dim).cloned()).unwrap_or(0.0);
                    let c1 = f2.c1.as_ref().and_then(|c1| c1.get(dim).cloned()).unwrap_or(1.0);
                    let exponent = f2.exponent;
                    parts.push(InterpolatedFunctionDim {
                        input_range, output_range, c0, c1, exponent
                    });
                }
                Ok(Function::Interpolated(parts))
            },
            _ => {
                dbg!(raw);
                unimplemented!()
            }
        }
    }
}

#[derive(Debug)]
pub struct InterpolatedFunctionDim {
    pub input_range: (f32, f32),
    pub output_range: (f32, f32),
    pub c0: f32,
    pub c1: f32,
    pub exponent: f32,
}
impl InterpolatedFunctionDim {
    pub fn apply(&self, x: f32) -> f32 {
        let y = self.c0 + x.powf(self.exponent) * (self.c1 - self.c0);
        let (y0, y1) = self.output_range;
        y.min(y1).max(y0)
    }
}
