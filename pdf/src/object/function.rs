use crate as pdf;
use crate::error::*;
use crate::object::*;

#[derive(Object, Debug, Clone)]
struct RawFunction {
    #[pdf(key = "FunctionType")]
    function_type: u32,

    #[pdf(key = "Domain")]
    domain: Vec<f32>,

    #[pdf(key = "Range")]
    range: Option<Vec<f32>>,

    #[pdf(other)]
    other: Dictionary,
}

#[derive(Object, Debug, Clone)]
struct Function2 {
    #[pdf(key = "C0")]
    c0: Option<Vec<f32>>,

    #[pdf(key = "C1")]
    c1: Option<Vec<f32>>,

    #[pdf(key = "N")]
    exponent: f32,
}

#[derive(Debug, Clone)]
pub enum Function {
    Sampled(SampledFunction),
    Interpolated(Vec<InterpolatedFunctionDim>),
    Stiching,
    Calculator,
    PostScript {
        func:   PsFunc,
        domain: Vec<f32>,
        range:  Vec<f32>,
    },
}
impl Function {
    pub fn apply(&self, x: &[f32], out: &mut [f32]) -> Result<()> {
        match *self {
            Function::Sampled(ref func) => func.apply(x, out),
            Function::Interpolated(ref parts) => {
                if parts.len() != out.len() {
                    bail!(
                        "incorrect output length: expected {}, found {}.",
                        parts.len(),
                        out.len()
                    )
                }
                for (f, y) in parts.iter().zip(out) {
                    *y = f.apply(x[0]);
                }
                Ok(())
            }
            Function::PostScript { ref func, .. } => func.exec(x, out),
            _ => bail!("unimplemted function {:?}", self),
        }
    }
    pub fn input_dim(&self) -> usize {
        match *self {
            Function::PostScript { ref domain, .. } => domain.len() / 2,
            _ => panic!(),
        }
    }
    pub fn output_dim(&self) -> usize {
        match *self {
            Function::PostScript { ref range, .. } => range.len() / 2,
            _ => panic!(),
        }
    }
}
impl FromDict for Function {
    fn from_dict(dict: Dictionary, resolve: &impl Resolve) -> Result<Self> {
        use std::f32::INFINITY;
        let raw = RawFunction::from_dict(dict, resolve)?;
        match raw.function_type {
            2 => {
                let f2 = Function2::from_dict(raw.other, resolve)?;
                let mut parts = Vec::with_capacity(raw.domain.len());

                let n_dim = match (raw.range.as_ref(), f2.c0.as_ref(), f2.c1.as_ref()) {
                    (Some(range), _, _) => range.len() / 2,
                    (_, Some(c0), _) => c0.len(),
                    (_, _, Some(c1)) => c1.len(),
                    _ => bail!("unknown dimensions"),
                };
                let input_range = (raw.domain[0], raw.domain[1]);
                for dim in 0..n_dim {
                    let output_range = (
                        raw.range
                            .as_ref()
                            .and_then(|r| r.get(2 * dim).cloned())
                            .unwrap_or(-INFINITY),
                        raw.range
                            .as_ref()
                            .and_then(|r| r.get(2 * dim + 1).cloned())
                            .unwrap_or(INFINITY),
                    );
                    let c0 = f2
                        .c0
                        .as_ref()
                        .and_then(|c0| c0.get(dim).cloned())
                        .unwrap_or(0.0);
                    let c1 = f2
                        .c1
                        .as_ref()
                        .and_then(|c1| c1.get(dim).cloned())
                        .unwrap_or(1.0);
                    let exponent = f2.exponent;
                    parts.push(InterpolatedFunctionDim {
                        input_range,
                        output_range,
                        c0,
                        c1,
                        exponent,
                    });
                }
                Ok(Function::Interpolated(parts))
            }
            i => {
                dbg!(raw);
                bail!("unsupported function type {}", i)
            }
        }
    }
}
impl Object for Function {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Dictionary(dict) => Self::from_dict(dict, resolve),
            Primitive::Stream(mut s) => {
                let stream = Stream::<RawFunction>::from_stream(s, resolve)?;
                let data = stream.decode()?;
                match stream.info.function_type {
                    4 => {
                        let s = std::str::from_utf8(&*data)?;
                        let func = PsFunc::parse(s)?;
                        let info = stream.info.info;
                        Ok(Function::PostScript {
                            func,
                            domain: info.domain,
                            range: info.range.unwrap(),
                        })
                    }
                    0 => Ok(Function::Sampled(SampledFunction {
                        input: vec![],
                        data:  vec![],
                        order: Interpolation::Linear,
                    })),
                    ref p => bail!("found a function stream with type {:?}", p),
                }
            }
            Primitive::Reference(r) => Self::from_primitive(resolve.resolve(r)?, resolve),
            _ => bail!("double indirection"),
        }
    }
}

#[derive(Debug, Clone)]
struct SampledFunctionInput {
    domain:        (f32, f32),
    encode_offset: f32,
    encode_scale:  f32,
    size:          u32,
}
impl SampledFunctionInput {
    fn map(&self, x: f32) -> f32 {
        let x = x.clamp(self.domain.0, self.domain.1);
        x.mul_add(self.encode_scale, self.encode_offset)
    }
}

#[derive(Debug, Clone)]
struct SampledFunctionOutput {
    output_offset: f32,
    output_scale:  f32,
}

#[derive(Debug, Clone)]
enum Interpolation {
    Linear,
    Cubic,
}

#[derive(Debug, Clone)]
pub struct SampledFunction {
    input: Vec<SampledFunctionInput>,
    data:  Vec<u8>,
    order: Interpolation,
}
impl SampledFunction {
    fn apply(&self, x: &[f32], _out: &mut [f32]) -> Result<()> {
        let _idx: Vec<f32> = x
            .iter()
            .zip(self.input.iter())
            .map(|(&x, dim)| dim.map(x))
            .collect();
        match self.order {
            Interpolation::Linear => {
                unimplemented!()
            }
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterpolatedFunctionDim {
    pub input_range:  (f32, f32),
    pub output_range: (f32, f32),
    pub c0:           f32,
    pub c1:           f32,
    pub exponent:     f32,
}
impl InterpolatedFunctionDim {
    pub fn apply(&self, x: f32) -> f32 {
        let y = self.c0 + x.powf(self.exponent) * (self.c1 - self.c0);
        let (y0, y1) = self.output_range;
        y.min(y1).max(y0)
    }
}

#[derive(Debug)]
pub enum PostScriptError {
    StackUnderflow,
    IncorrectStackSize,
}
#[derive(Debug, Clone)]
pub struct PsFunc {
    pub ops: Vec<PsOp>,
}

macro_rules! op {
    ($stack:ident; $($v:ident),* => $($e:expr),*) => ( {
        $(let $v = $stack.pop().ok_or(PostScriptError::StackUnderflow)?;)*
        $($stack.push($e);)*
    } )
}

impl PsFunc {
    fn exec_inner(&self, stack: &mut Vec<f32>) -> Result<(), PostScriptError> {
        for &op in &self.ops {
            match op {
                PsOp::Int(i) => stack.push(i as f32),
                PsOp::Value(v) => stack.push(v),
                PsOp::Dup => op!(stack; v => v, v),
                PsOp::Exch => op!(stack; a, b => a, b),
                PsOp::Add => op!(stack; a, b => a + b),
                PsOp::Sub => op!(stack; a, b => a - b),
                PsOp::Mul => op!(stack; a, b => a * b),
                PsOp::Abs => op!(stack; a => a.abs()),
                PsOp::Roll => {
                    let j = stack.pop().ok_or(PostScriptError::StackUnderflow)? as isize;
                    let n = stack.pop().ok_or(PostScriptError::StackUnderflow)? as usize;
                    let start = stack.len() - n;
                    let slice = &mut stack[start..];
                    if j > 0 {
                        slice.rotate_right(j as usize);
                    } else {
                        slice.rotate_left(-j as usize);
                    }
                }
                PsOp::Index => {
                    let n = stack.pop().ok_or(PostScriptError::StackUnderflow)? as usize;
                    if n >= stack.len() {
                        return Err(PostScriptError::StackUnderflow);
                    }
                    let val = stack[stack.len() - n - 1];
                    stack.push(val);
                }
                PsOp::Cvr => {}
                PsOp::Pop => {
                    stack.pop().ok_or(PostScriptError::StackUnderflow)?;
                }
            }
        }
        Ok(())
    }
    pub fn exec(&self, input: &[f32], output: &mut [f32]) -> Result<()> {
        let mut stack = Vec::with_capacity(10);
        stack.extend_from_slice(input);
        match self.exec_inner(&mut stack) {
            Ok(()) => {}
            Err(_) => return Err(PdfError::PostScriptExec),
        }
        if output.len() != stack.len() {
            bail!(
                "incorrect output length: expected {}, found {}.",
                stack.len(),
                output.len()
            )
        }
        output.copy_from_slice(&stack);
        Ok(())
    }
    pub fn parse(s: &str) -> Result<Self, PdfError> {
        let start = s.find("{").ok_or(PdfError::PostScriptParse)?;
        let end = s.rfind("}").ok_or(PdfError::PostScriptParse)?;

        let ops: Result<Vec<_>, _> = s[start + 1..end]
            .split_ascii_whitespace()
            .map(PsOp::parse)
            .collect();
        Ok(PsFunc { ops: ops? })
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PsOp {
    Int(i32),
    Value(f32),
    Add,
    Sub,
    Abs,
    Mul,
    Dup,
    Exch,
    Roll,
    Index,
    Cvr,
    Pop,
}
impl PsOp {
    pub fn parse(s: &str) -> Result<Self> {
        if let Ok(i) = s.parse::<i32>() {
            Ok(PsOp::Int(i))
        } else if let Ok(f) = s.parse::<f32>() {
            Ok(PsOp::Value(f))
        } else {
            Ok(match s {
                "add" => PsOp::Add,
                "sub" => PsOp::Sub,
                "abs" => PsOp::Abs,
                "mul" => PsOp::Mul,
                "dup" => PsOp::Dup,
                "exch" => PsOp::Exch,
                "roll" => PsOp::Roll,
                "index" => PsOp::Index,
                "cvr" => PsOp::Cvr,
                "pop" => PsOp::Pop,
                _ => {
                    bail!("unimplemented op {}", s);
                }
            })
        }
    }
}
