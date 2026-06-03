use crate as pdf;
use crate::error::*;
use crate::object::*;
use datasize::DataSize;
use itertools::izip;

#[derive(Object, Debug, Clone, ObjectWrite)]
struct RawFunction {
    #[pdf(key = "FunctionType")]
    function_type: u32,

    #[pdf(key = "Domain")]
    domain: Vec<f32>,

    #[pdf(key = "Range")]
    range: Option<Vec<f32>>,

    #[pdf(key = "Size")]
    size: Option<Vec<u32>>,

    #[pdf(key = "BitsPerSample")]
    _bits_per_sample: Option<u32>,

    #[pdf(key = "Order", default = "1")]
    order: u32,

    #[pdf(key = "Encode")]
    encode: Option<Vec<f32>>,

    #[pdf(key = "Decode")]
    decode: Option<Vec<f32>>,

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

#[derive(Object, Debug, Clone)]
struct Function3 {
    #[pdf(key = "Functions")]
    functions: Vec<Function>,

    #[pdf(key = "Bounds")]
    bounds: Vec<f32>,
}

/// A Type 3 (stitching) function (PDF 32000-1 §7.10.4): it partitions its
/// one-dimensional `domain` with `bounds` and delegates each subinterval to one
/// of the `functions`, remapping the input through `encode`.
#[derive(Debug, Clone, DataSize)]
pub struct StitchingFunction {
    domain: Vec<f32>,
    functions: Vec<Function>,
    bounds: Vec<f32>,
    encode: Vec<f32>,
}
impl StitchingFunction {
    fn output_dim(&self) -> usize {
        self.functions.first().map_or(0, Function::output_dim)
    }
    fn apply(&self, x: &[f32], out: &mut [f32]) -> Result<()> {
        let k = self.functions.len();
        if k == 0 {
            bail!("stitching function has no subfunctions");
        }
        let d0 = self.domain.first().copied().unwrap_or(0.0);
        let d1 = self.domain.get(1).copied().unwrap_or(1.0);
        let t = x.first().copied().unwrap_or(0.0).clamp(d0.min(d1), d0.max(d1));
        // With k functions there are k-1 bounds; the subinterval index is how
        // many bounds `t` has reached, landing in 0..k.
        let i = self.bounds.iter().take_while(|&&b| t >= b).count().min(k - 1);
        let lo = if i == 0 { d0 } else { self.bounds[i - 1] };
        let hi = self.bounds.get(i).copied().unwrap_or(d1);
        let e0 = self.encode.get(2 * i).copied().unwrap_or(0.0);
        let e1 = self.encode.get(2 * i + 1).copied().unwrap_or(1.0);
        let mapped = if (hi - lo).abs() > f32::EPSILON {
            e0 + (t - lo) * (e1 - e0) / (hi - lo)
        } else {
            e0
        };
        self.functions[i].apply(&[mapped], out)
    }
}

#[derive(Debug, Clone, DataSize)]
pub enum Function {
    Sampled(SampledFunction),
    Interpolated(Vec<InterpolatedFunctionDim>),
    Stitching(StitchingFunction),
    Calculator,
    PostScript {
        func: PsFunc,
        domain: Vec<f32>,
        range: Vec<f32>,
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
            Function::Stitching(ref func) => func.apply(x, out),
            _ => bail!("unimplemted function {:?}", self),
        }
    }
    pub fn input_dim(&self) -> usize {
        match *self {
            Function::PostScript { ref domain, .. } => domain.len() / 2,
            Function::Sampled(ref f) => f.input.len(),
            // Stitching and exponential-interpolation functions take one input.
            Function::Stitching(_) | Function::Interpolated(_) => 1,
            _ => panic!(),
        }
    }
    pub fn output_dim(&self) -> usize {
        match *self {
            Function::PostScript { ref range, .. } => range.len() / 2,
            Function::Sampled(ref f) => f.output.len(),
            Function::Interpolated(ref parts) => parts.len(),
            Function::Stitching(ref f) => f.output_dim(),
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

                let n_dim = match (raw.range.as_ref(), f2.c0.as_ref(), f2.c1.as_ref()) {
                    (Some(range), _, _) => range.len() / 2,
                    (_, Some(c0), _) => c0.len(),
                    (_, _, Some(c1)) => c1.len(),
                    _ => bail!("unknown dimensions"),
                };
                let mut parts = Vec::with_capacity(n_dim);
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
            3 => {
                // `Encode`/`Domain` are captured by `RawFunction`; `Functions`
                // and `Bounds` fall through to the catch-all dictionary.
                let f3 = Function3::from_dict(raw.other, resolve)?;
                Ok(Function::Stitching(StitchingFunction {
                    domain: raw.domain,
                    functions: f3.functions,
                    bounds: f3.bounds,
                    encode: raw.encode.unwrap_or_default(),
                }))
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
            Primitive::Stream(s) => {
                let stream = Stream::<RawFunction>::from_stream(s, resolve)?;
                let data = stream.data(resolve)?;
                match stream.info.function_type {
                    4 => {
                        let s = std::str::from_utf8(&data)?;
                        let func = PsFunc::parse(s)?;
                        let info = stream.info.info;
                        Ok(Function::PostScript {
                            func,
                            domain: info.domain,
                            range: info.range.unwrap(),
                        })
                    }
                    0 => {
                        let info = stream.info.info;
                        let order = match info.order {
                            1 => Interpolation::Linear,
                            3 => Interpolation::Cubic,
                            n => bail!("Invalid interpolation order {}", n),
                        };

                        let size = try_opt!(info.size);
                        let range = try_opt!(info.range);
                        let encode = info.encode.unwrap_or_else(|| {
                            size.iter().flat_map(|&n| [0.0, (n - 1) as f32]).collect()
                        });
                        let decode = info.decode.unwrap_or_else(|| range.clone());

                        Ok(Function::Sampled(SampledFunction {
                            input: izip!(
                                info.domain.chunks_exact(2),
                                encode.chunks_exact(2),
                                size.iter()
                            )
                            .map(|(c, e, &s)| SampledFunctionInput {
                                domain: (c[0], c[1]),
                                encode_offset: e[0],
                                encode_scale: e[1],
                                size: s as usize,
                            })
                            .collect(),
                            output: decode
                                .chunks_exact(2)
                                .map(|c| SampledFunctionOutput {
                                    offset: c[0],
                                    scale: (c[1] - c[0]) / 255.,
                                })
                                .collect(),
                            data,
                            order,
                            range,
                        }))
                    }
                    ref p => bail!("found a function stream with type {:?}", p),
                }
            }
            Primitive::Reference(r) => Self::from_primitive(resolve.resolve(r)?, resolve),
            _ => bail!("double indirection"),
        }
    }
}
impl ObjectWrite for Function {
    fn to_primitive(&self, _update: &mut impl Updater) -> Result<Primitive> {
        unimplemented!()
        /*
        let dict = match self {
            Function::Interpolated(parts) => {
                let first: &InterpolatedFunctionDim = try_opt!(parts.get(0));
                let f2 = Function2 {
                    c0: parts.iter().map(|p| p.c0).collect(),
                    c1: parts.iter().map(|p| p.c0).collect(),
                    exponent: first.exponent
                };
                let f = RawFunction {
                    function_type: 2,
                    domain: vec![first.input_range.0, first.input_range.1],
                    range: parts.iter().flat_map(|p| [p.output_range.0, p.output_range.1]).collect(),
                    decode: None,
                    encode: None,
                    order
                };

            }
        }
        */
    }
}
impl DeepClone for Function {
    fn deep_clone(&self, _cloner: &mut impl Cloner) -> Result<Self> {
        Ok(self.clone())
    }
}

#[derive(Debug, Clone, DataSize)]
struct SampledFunctionInput {
    domain: (f32, f32),
    encode_offset: f32,
    encode_scale: f32,
    size: usize,
}
impl SampledFunctionInput {
    fn map(&self, x: f32) -> (usize, usize, f32) {
        let x = x.clamp(self.domain.0, self.domain.1);
        let y = x.mul_add(self.encode_scale, self.encode_offset);
        (y.floor() as usize, self.size, y.fract())
    }
}

#[derive(Debug, Clone, DataSize)]
struct SampledFunctionOutput {
    offset: f32,
    scale: f32,
}
impl SampledFunctionOutput {
    fn map(&self, x: f32) -> f32 {
        x.mul_add(self.scale, self.offset)
    }
}

#[derive(Debug, Clone, DataSize)]
enum Interpolation {
    Linear,
    #[allow(dead_code)] // TODO
    Cubic,
}

#[derive(Debug, Clone, DataSize)]
pub struct SampledFunction {
    input: Vec<SampledFunctionInput>,
    output: Vec<SampledFunctionOutput>,
    data: Arc<[u8]>,
    order: Interpolation,
    range: Vec<f32>,
}
impl SampledFunction {
    fn apply(&self, x: &[f32], out: &mut [f32]) -> Result<()> {
        if x.len() != self.input.len() {
            bail!(
                "input dimension mismatch {} != {}",
                x.len(),
                self.input.len()
            );
        }
        let n_out = out.len();
        if out.len() * 2 != self.range.len() {
            bail!(
                "output dimension mismatch 2 * {} != {}",
                out.len(),
                self.range.len()
            )
        }
        match x.len() {
            1 => match self.order {
                Interpolation::Linear => {
                    let (i, _, s) = self.input[0].map(x[0]);
                    let idx = i * n_out;

                    for (o, &a) in out.iter_mut().zip(&self.data[idx..]) {
                        *o = a as f32 * (1. - s);
                    }
                    for (o, &b) in out.iter_mut().zip(&self.data[idx + n_out..]) {
                        *o += b as f32 * s;
                    }
                }
                _ => unimplemented!(),
            },
            2 => match self.order {
                Interpolation::Linear => {
                    let (i0, s0, f0) = self.input[0].map(x[0]);
                    let (i1, _, f1) = self.input[1].map(x[1]);
                    let (j0, j1) = (i0 + 1, i1 + 1);
                    let (g0, g1) = (1. - f0, 1. - f1);

                    out.fill(0.0);
                    let mut add = |i0, i1, f| {
                        let idx = (i0 + s0 * i1) * n_out;

                        if let Some(part) = self.data.get(idx..idx + n_out) {
                            for (o, &b) in out.iter_mut().zip(part) {
                                *o += f * b as f32;
                            }
                        }
                    };

                    add(i0, i1, g0 * g1);
                    add(j0, i1, f0 * g1);
                    add(i0, j1, g0 * f1);
                    add(j0, j1, f0 * f1);
                }
                _ => unimplemented!(),
            },
            3 => match self.order {
                Interpolation::Linear => {
                    let (i0, s0, f0) = self.input[0].map(x[0]);
                    let (i1, s1, f1) = self.input[1].map(x[1]);
                    let (i2, _, f2) = self.input[2].map(x[2]);
                    let (j0, j1, j2) = (i0 + 1, i1 + 1, i2 + 1);
                    let (g0, g1, g2) = (1. - f0, 1. - f1, 1. - f2);

                    out.fill(0.0);
                    let mut add = |i0, i1, i2, f| {
                        let idx = (i0 + s0 * (i1 + s1 * i2)) * n_out;

                        if let Some(part) = self.data.get(idx..idx + n_out) {
                            for (o, &b) in out.iter_mut().zip(part) {
                                *o += f * b as f32;
                            }
                        }
                    };

                    add(i0, i1, i2, g0 * g1 * g2);
                    add(j0, i1, i2, f0 * g1 * g2);
                    add(i0, j1, i2, g0 * f1 * g2);
                    add(j0, j1, i2, f0 * f1 * g2);

                    add(i0, i1, j2, g0 * g1 * f2);
                    add(j0, i1, j2, f0 * g1 * f2);
                    add(i0, j1, j2, g0 * f1 * f2);
                    add(j0, j1, j2, f0 * f1 * f2);
                }
                _ => unimplemented!(),
            },
            n => bail!("Order {}", n),
        }
        for (o, y) in self.output.iter().zip(out.iter_mut()) {
            *y = o.map(*y);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, DataSize)]
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

#[derive(Debug)]
pub enum PostScriptError {
    StackUnderflow,
    IncorrectStackSize,
}
#[derive(Debug, Clone, DataSize)]
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
                PsOp::Exch => op!(stack; b, a => b, a),
                PsOp::Add => op!(stack; b, a => a + b),
                PsOp::Sub => op!(stack; b, a => a - b),
                PsOp::Mul => op!(stack; b, a => a * b),
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
        let start = s.find('{').ok_or(PdfError::PostScriptParse)?;
        let end = s.rfind('}').ok_or(PdfError::PostScriptParse)?;

        let ops: Result<Vec<_>, _> = s[start + 1..end]
            .split_ascii_whitespace()
            .map(PsOp::parse)
            .collect();
        Ok(PsFunc { ops: ops? })
    }
}

#[derive(Copy, Clone, Debug, DataSize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::NoResolve;
    use crate::primitive::{Dictionary, Primitive};

    fn nums(v: &[f32]) -> Primitive {
        Primitive::Array(v.iter().map(|&x| Primitive::Number(x)).collect())
    }

    // A Type 2 exponential function mapping t in [0,1] to c0 + t*(c1-c0).
    fn exp_fn(c0: f32, c1: f32) -> Primitive {
        let mut d = Dictionary::new();
        d.insert("FunctionType", Primitive::Integer(2));
        d.insert("Domain", nums(&[0.0, 1.0]));
        d.insert("C0", nums(&[c0]));
        d.insert("C1", nums(&[c1]));
        d.insert("N", Primitive::Number(1.0));
        Primitive::Dictionary(d)
    }

    #[test]
    fn stitching_function_evaluates() {
        // Two sub-functions split at 0.5; each subinterval is encoded to [0,1].
        let mut d = Dictionary::new();
        d.insert("FunctionType", Primitive::Integer(3));
        d.insert("Domain", nums(&[0.0, 1.0]));
        d.insert("Functions", Primitive::Array(vec![exp_fn(0.0, 1.0), exp_fn(1.0, 2.0)]));
        d.insert("Bounds", nums(&[0.5]));
        d.insert("Encode", nums(&[0.0, 1.0, 0.0, 1.0]));

        let f = Function::from_primitive(Primitive::Dictionary(d), &NoResolve).unwrap();
        assert!(matches!(f, Function::Stitching(_)));
        assert_eq!(f.input_dim(), 1);
        assert_eq!(f.output_dim(), 1);

        let mut out = [0.0];
        // t=0.25 → first interval [0,0.5] → encoded to 0.5 → exp(0,1)@0.5 = 0.5
        f.apply(&[0.25], &mut out).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-4, "got {}", out[0]);
        // t=0.75 → second interval [0.5,1] → encoded to 0.5 → exp(1,2)@0.5 = 1.5
        f.apply(&[0.75], &mut out).unwrap();
        assert!((out[0] - 1.5).abs() < 1e-4, "got {}", out[0]);
        // Endpoints stay within range.
        f.apply(&[0.0], &mut out).unwrap();
        assert!((out[0] - 0.0).abs() < 1e-4, "got {}", out[0]);
        f.apply(&[1.0], &mut out).unwrap();
        assert!((out[0] - 2.0).abs() < 1e-4, "got {}", out[0]);
    }
}
