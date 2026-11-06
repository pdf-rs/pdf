use crate as pdf;
use crate::error::*;
use crate::object::*;
use datasize::DataSize;

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
    bits_per_sample: Option<u32>,

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
            Function::Sampled(ref f) => f.output_dim,
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

                        Ok(Function::Sampled(sampled::new(
                            info.domain,
                            try_opt!(info.size),
                            info.encode,
                            info.decode,
                            try_opt!(info.range),
                            info.bits_per_sample.unwrap_or(8),
                            order,
                            &data,
                        )?))
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
        // Not implemented: a function is kept as its evaluated form for
        // shading, and writing one back would need the original /Range,
        // /Domain and sample stream. Refusing is the behaviour a caller can
        // handle; this used to be a panic in the middle of a save.
        bail!("writing a function object back to a primitive is not implemented")
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
    /// `y = x * encode_scale + encode_offset` is 7.10.2's
    /// `Interpolate(x, Domain0, Domain1, Encode0, Encode1)`.
    encode_scale: f32,
    encode_offset: f32,
    size: usize,
}
impl SampledFunctionInput {
    /// The grid cell and the position inside it, or `(0, 0.0)` for a
    /// degenerate one-sample dimension.
    fn position(&self, x: f32) -> (usize, f32) {
        // /Domain is not validated, and f32::clamp panics if min > max.
        let (d0, d1) = self.domain;
        let x = if x.is_nan() {
            d0
        } else {
            x.clamp(d0.min(d1), d0.max(d1))
        };
        let scale = if self.encode_scale.is_finite() { self.encode_scale } else { 0.0 };
        let y = x.mul_add(scale, self.encode_offset);
        // The encoded value is clipped to the sample grid. /Encode is not validated and
        // may be infinite or NaN, which would otherwise give out-of-range indices.
        let last = self.size.saturating_sub(1);
        let y = if y.is_nan() || y <= 0.0 {
            0.0
        } else {
            y.clamp(0.0, last as f32)
        };
        let i = (y.floor() as usize).min(last);
        (i, y - i as f32)
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
    /// Number of output values per sample (pairs in `/Range`).
    output_dim: usize,
    /// Grid stride of each input dimension; dimension 0 varies fastest (7.10.2).
    strides: Vec<usize>,
    /// Every sample, already mapped through `/Decode` and clipped to `/Range`.
    samples: Vec<f32>,
    order: Interpolation,
}
impl SampledFunction {
    fn apply(&self, x: &[f32], out: &mut [f32]) -> Result<()> {
        // Decoding and interpolation live in `sampled`, next to the
        // construction that fills in the decoded samples.
        sampled::apply(self, x, out)
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
                    let start = stack.len().checked_sub(n).ok_or(PostScriptError::StackUnderflow)?;
                    let slice = stack.get_mut(start..).ok_or(PostScriptError::IncorrectStackSize)?;
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
    use crate::primitive::{Dictionary, PdfStream, Primitive, StreamInner};

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

    fn sampled_1d(encode: &[f32]) -> Function {
        let mut d = Dictionary::new();
        d.insert("FunctionType", Primitive::Integer(0));
        d.insert("Domain", nums(&[0.0, 1.0]));
        d.insert("Range", nums(&[0.0, 1.0, 0.0, 1.0]));
        d.insert("Encode", nums(encode));
        d.insert("Size", Primitive::Array(vec![Primitive::Integer(2)]));
        d.insert("BitsPerSample", Primitive::Integer(8));
        d.insert("Length", Primitive::Integer(4));
        let stream = PdfStream {
            info: d,
            inner: StreamInner::Pending {
                data: Arc::from(&[0u8, 0, 255, 255][..]),
            },
        };
        Function::from_primitive(Primitive::Stream(stream), &NoResolve).unwrap()
    }

    // #289: a non-finite /Encode used to overflow the sample index.
    #[test]
    fn sampled_function_with_non_finite_encode() {
        for encode in [
            [0.0, f32::INFINITY],
            [f32::INFINITY, f32::INFINITY],
            [f32::NEG_INFINITY, 1.0],
            [f32::NAN, 1.0],
            [0.0, f32::NAN],
        ] {
            let f = sampled_1d(&encode);
            let mut out = [0.0; 2];
            for x in [0.0, 0.5, 1.0] {
                f.apply(&[x], &mut out).unwrap();
                assert!(out.iter().all(|y| (0.0..=1.0).contains(y)), "{encode:?} x={x}: {out:?}");
            }
        }
    }

    #[test]
    fn sampled_function_with_inverted_domain() {
        // A two-sample ramp from 0 to 1 whose /Domain is written high to low.
        // Evaluating it used to panic in f32::clamp.
        let mut d = Dictionary::new();
        d.insert("FunctionType", Primitive::Integer(0));
        d.insert("Domain", nums(&[1.0, 0.0]));
        d.insert("Range", nums(&[0.0, 1.0]));
        d.insert("Size", Primitive::Array(vec![Primitive::Integer(2)]));
        d.insert("BitsPerSample", Primitive::Integer(8));
        d.insert("Length", Primitive::Integer(2));
        let stream = PdfStream {
            info: d,
            inner: StreamInner::Pending {
                data: Arc::from(&[0u8, 255][..]),
            },
        };

        let f = Function::from_primitive(Primitive::Stream(stream), &NoResolve).unwrap();
        let mut out = [0.0];
        for x in [-1.0, 0.0, 0.25, 0.5, 1.0, 2.0] {
            f.apply(&[x], &mut out).unwrap();
            let want = x.clamp(0.0, 1.0);
            assert!((out[0] - want).abs() < 1e-4, "x = {x}: got {}", out[0]);
        }
    }

    fn sampled_stream(entries: &[(&str, Primitive)], data: &[u8]) -> PdfStream {
        let mut d = Dictionary::new();
        d.insert("FunctionType", Primitive::Integer(0));
        for (key, value) in entries {
            d.insert(*key, value.clone());
        }
        d.insert("Length", Primitive::Integer(data.len() as i64));
        PdfStream {
            info: d,
            inner: StreamInner::Pending {
                data: Arc::from(data),
            },
        }
    }

    fn sampled_function(entries: &[(&str, Primitive)], data: &[u8]) -> Result<Function> {
        Function::from_primitive(Primitive::Stream(sampled_stream(entries, data)), &NoResolve)
    }

    fn size(sizes: &[i64]) -> Primitive {
        Primitive::Array(sizes.iter().map(|&n| Primitive::Integer(n)).collect())
    }

    // 7.10.2: the samples are a continuous bit stream of /BitsPerSample bits
    // each. The old code indexed the raw bytes, so any other width read garbage.
    #[test]
    fn sampled_function_reads_the_configured_bits_per_sample() {
        // Four 4-bit samples 0, 5, 10, 15 in two bytes; /Decode defaults to
        // /Range and maps them to 0, 5/15, 10/15, 1.
        let f = sampled_function(
            &[
                ("Domain", nums(&[0.0, 3.0])),
                ("Size", size(&[4])),
                ("BitsPerSample", Primitive::Integer(4)),
                ("Range", nums(&[0.0, 1.0])),
            ],
            &[0x05, 0xAF],
        )
        .unwrap();
        let mut out = [0.0];
        for (x, want) in [(0.0, 0.0), (1.0, 5.0 / 15.0), (2.0, 10.0 / 15.0), (3.0, 1.0)] {
            f.apply(&[x], &mut out).unwrap();
            assert!((out[0] - want).abs() < 1e-4, "x = {x}: got {}, want {want}", out[0]);
        }
    }

    // 7.10.2: the encoding interpolates the domain onto the sample grid:
    // `Encode0 + (x - Domain0) * (Encode1 - Encode0) / (Domain1 - Domain0)`.
    // The old scale was `Encode1` alone, right only for a domain of width 1.
    #[test]
    fn sampled_function_encodes_over_the_whole_domain() {
        let f = sampled_function(
            &[
                ("Domain", nums(&[-1.0, 1.0])),
                ("Size", size(&[2])),
                ("BitsPerSample", Primitive::Integer(8)),
                ("Range", nums(&[0.0, 1.0])),
            ],
            &[0, 255],
        )
        .unwrap();
        let mut out = [0.0];
        // x = 0 sits halfway between the samples; the old code mapped it to 0.
        f.apply(&[0.0], &mut out).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-3, "got {}", out[0]);
        f.apply(&[-1.0], &mut out).unwrap();
        assert!(out[0].abs() < 1e-4, "got {}", out[0]);
        f.apply(&[1.0], &mut out).unwrap();
        assert!((out[0] - 1.0).abs() < 1e-4, "got {}", out[0]);
    }

    // /Order 3 used to be an error, which dropped the whole shading.
    #[test]
    fn sampled_function_with_cubic_spline() {
        // 0, 1, 0, 1, 0 sampled at x = 0 ..= 4, so the curve oscillates and a
        // cubic is distinguishable from the linear join.
        let f = sampled_function(
            &[
                ("Domain", nums(&[0.0, 4.0])),
                ("Size", size(&[5])),
                ("BitsPerSample", Primitive::Integer(8)),
                ("Range", nums(&[0.0, 1.0])),
                ("Order", Primitive::Integer(3)),
            ],
            &[0, 255, 0, 255, 0],
        )
        .unwrap();
        let mut out = [0.0];
        // The spline passes through every sample ...
        for (x, want) in [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0), (4.0, 0.0)] {
            f.apply(&[x], &mut out).unwrap();
            assert!((out[0] - want).abs() < 1e-4, "x = {x}: got {}, want {want}", out[0]);
        }
        // ... and between two knots it is not the linear join: Catmull–Rom at
        // x = 1.25 over the samples 0, 1, 0, 1 gives 0.84375, the linear join
        // gives 0.75.
        f.apply(&[1.25], &mut out).unwrap();
        assert!((out[0] - 0.84375).abs() < 1e-4, "got {}", out[0]);
    }

    // 7.10.2: "If Size is less than 4, cubic spline interpolation is not
    // possible and Order 3 shall be ignored if specified."
    #[test]
    fn sampled_function_ignores_cubic_when_a_size_is_smaller_than_four() {
        let f = sampled_function(
            &[
                ("Domain", nums(&[0.0, 2.0])),
                ("Size", size(&[3])),
                ("BitsPerSample", Primitive::Integer(8)),
                ("Range", nums(&[0.0, 1.0])),
                ("Order", Primitive::Integer(3)),
            ],
            &[0, 255, 0],
        )
        .unwrap();
        let mut out = [0.0];
        f.apply(&[0.5], &mut out).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-4, "got {}", out[0]);
    }

    // 7.10.2: "The stream data shall be long enough to contain the entire
    // sample array." A short stream used to be read as whatever bytes the
    // interpolation happened to reach.
    #[test]
    fn sampled_function_rejects_a_short_stream() {
        let err = sampled_function(
            &[
                ("Domain", nums(&[0.0, 3.0])),
                ("Size", size(&[4])),
                ("BitsPerSample", Primitive::Integer(8)),
                ("Range", nums(&[0.0, 1.0])),
            ],
            &[0, 255],
        )
        .err()
        .expect("a stream with four samples needs four bytes");
        assert!(err.to_string().contains("needs 4 bytes"), "{err}");
    }

    #[test]
    fn sampled_function_rejects_unsupported_bits_per_sample() {
        let err = sampled_function(
            &[
                ("Domain", nums(&[0.0, 1.0])),
                ("Size", size(&[2])),
                ("BitsPerSample", Primitive::Integer(3)),
                ("Range", nums(&[0.0, 1.0])),
            ],
            &[0, 255, 0, 0, 0, 0],
        )
        .err()
        .expect("/BitsPerSample 3 is not a legal width");
        assert!(err.to_string().contains("BitsPerSample"), "{err}");
    }
}

/// Decoding and evaluating type 0 (sampled) functions (7.10.2).
///
/// The samples are decoded once, when the function is built, from the bit
/// stream into `f32` values that are already mapped through `/Decode` into
/// `/Range`. Interpolation then works on plain numbers, which is what makes
/// `/BitsPerSample` and cubic splines tractable: the old code treated the raw
/// bytes as samples (so anything but 8 bits per sample read garbage) and had a
/// separate interpolation body per input dimension.
mod sampled {
    use super::*;

    /// Decoded samples are held as `f32`, so a 1-bit sample array grows
    /// eightfold. 4 Mi values (16 MiB) is far beyond any real function, and it
    /// keeps a hostile `/Size` from turning a small stream into a huge
    /// allocation.
    const MAX_DECODED_SAMPLES: usize = 1 << 22;

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        domain: Vec<f32>,
        size: Vec<u32>,
        encode: Option<Vec<f32>>,
        decode: Option<Vec<f32>>,
        range: Vec<f32>,
        bits_per_sample: u32,
        order: Interpolation,
        data: &[u8],
    ) -> Result<SampledFunction> {
        if size.is_empty() || domain.len() != 2 * size.len() {
            bail!(
                "/Domain has {} numbers for {} /Size entries",
                domain.len(),
                size.len()
            );
        }
        if range.is_empty() || range.len() % 2 != 0 {
            bail!("/Range must hold pairs of numbers, found {} values", range.len());
        }
        if !matches!(bits_per_sample, 1 | 2 | 4 | 8 | 12 | 16) {
            bail!(
                "/BitsPerSample {} is not one of 1, 2, 4, 8, 12, 16",
                bits_per_sample
            );
        }
        let output_dim = range.len() / 2;

        let mut total: usize = 1;
        for &n in &size {
            let n = n as usize;
            if n == 0 {
                bail!("/Size contains a zero");
            }
            total = match total.checked_mul(n) {
                Some(t) => t,
                None => bail!("/Size {:?} overflows", size),
            };
        }
        let n_values = match total.checked_mul(output_dim) {
            Some(n) if n <= MAX_DECODED_SAMPLES => n,
            _ => bail!(
                "/Size {:?} with {} outputs per sample is more than this implementation decodes",
                size,
                output_dim
            ),
        };
        // 7.10.2: "The stream data shall be long enough to contain the entire
        // sample array." A short stream used to be read as whatever bytes the
        // interpolation happened to index, silently shifting the grid.
        let bits_needed = (n_values as u64) * (bits_per_sample as u64);
        if (data.len() as u64) * 8 < bits_needed {
            bail!(
                "a sampled function needs {} bytes of samples, the stream has {}",
                (bits_needed + 7) / 8,
                data.len()
            );
        }

        let encode = encode.unwrap_or_else(|| {
            size.iter()
                .flat_map(|&n| [0.0, n.saturating_sub(1) as f32])
                .collect::<Vec<f32>>()
        });
        let decode = decode.unwrap_or_else(|| range.clone());

        let mut input = Vec::with_capacity(size.len());
        let mut strides = Vec::with_capacity(size.len());
        let mut stride = 1usize;
        for (dim, &n) in size.iter().enumerate() {
            let (d0, d1) = (domain[2 * dim], domain[2 * dim + 1]);
            let last = n.saturating_sub(1) as f32;
            let e0 = encode.get(2 * dim).copied().unwrap_or(0.0);
            let e1 = encode.get(2 * dim + 1).copied().unwrap_or(last);
            // 7.10.2 defines the encoding as
            // `Interpolate(x, Domain0, Domain1, Encode0, Encode1)`, i.e. the
            // *pair* of encode values over the width of the domain. The old code
            // took `Encode1` as the scale, which is only right for a domain of
            // width 1 (a domain [-1 1] encoded to [0 20] was stretched 20x
            // instead of 10x and started at 0 instead of 10).
            // The spec requires Domain0 < Domain1; when a file writes them the
            // other way round the domain is normalised, so the function stays
            // monotone instead of coming out mirrored.
            let (d0, d1) = if d0 <= d1 { (d0, d1) } else { (d1, d0) };
            let width = d1 - d0;
            let (scale, offset) = if width.abs() > f32::EPSILON && e0.is_finite() && e1.is_finite()
            {
                let scale = (e1 - e0) / width;
                (scale, e0 - d0 * scale)
            } else {
                // A degenerate or non-finite domain maps every input to Encode0.
                (0.0, if e0.is_finite() { e0 } else { 0.0 })
            };
            input.push(SampledFunctionInput {
                domain: (d0, d1),
                encode_scale: scale,
                encode_offset: offset,
                size: n as usize,
            });
            strides.push(stride);
            stride = stride.saturating_mul(n as usize);
        }

        let max_sample = ((1u32 << bits_per_sample) - 1) as f32;
        let mut samples = Vec::with_capacity(n_values);
        for k in 0..n_values {
            let channel = k % output_dim;
            let (r0, r1) = (range[2 * channel], range[2 * channel + 1]);
            let (d0, d1) = (
                decode.get(2 * channel).copied().unwrap_or(r0),
                decode.get(2 * channel + 1).copied().unwrap_or(r1),
            );
            let raw = read_bits(data, k * bits_per_sample as usize, bits_per_sample) as f32;
            let value = d0 + raw * (d1 - d0) / max_sample;
            let value = if value.is_finite() { value } else { 0.0 };
            samples.push(if r0.is_finite() && r1.is_finite() {
                value.clamp(r0.min(r1), r0.max(r1))
            } else {
                value
            });
        }

        // 7.10.2: "If Size is less than 4, cubic spline interpolation is not
        // possible and Order 3 shall be ignored if specified."
        let order = match order {
            Interpolation::Cubic if size.iter().any(|&n| n < 4) => Interpolation::Linear,
            o => o,
        };

        Ok(SampledFunction {
            input,
            output_dim,
            strides,
            samples,
            order,
        })
    }

    /// `bits` bits starting at `start_bit`, high-order bit of each byte first
    /// (7.10.2). The caller has already checked that the data is long enough.
    fn read_bits(data: &[u8], start_bit: usize, bits: u32) -> u32 {
        let mut value = 0u32;
        for k in 0..bits as usize {
            let bit = start_bit + k;
            let byte = data[bit / 8];
            value = (value << 1) | u32::from((byte >> (7 - (bit % 8))) & 1);
        }
        value
    }

    /// Catmull–Rom weights for the four grid points `i-1 ..= i+2` around the
    /// position `s` in `[0, 1]` between grid points `i` and `i + 1`. The curve
    /// passes through every sample, which is what 7.10.2's cubic spline
    /// interpolation has to do; the spec leaves the spline itself open, and
    /// this is the uniform choice that reduces to linear when the neighbour
    /// gaps close. The weights sum to 1.
    fn cubic_weights(s: f32) -> [f32; 4] {
        let s2 = s * s;
        let s3 = s2 * s;
        [
            -0.5 * s + s2 - 0.5 * s3,
            1.0 - 2.5 * s2 + 1.5 * s3,
            0.5 * s + 2.0 * s2 - 1.5 * s3,
            -0.5 * s2 + 0.5 * s3,
        ]
    }

    pub(super) fn apply(f: &SampledFunction, x: &[f32], out: &mut [f32]) -> Result<()> {
        if x.len() != f.input.len() {
            bail!(
                "input dimension mismatch {} != {}",
                x.len(),
                f.input.len()
            );
        }
        if out.len() != f.output_dim {
            bail!(
                "output dimension mismatch {} != {}",
                out.len(),
                f.output_dim
            );
        }

        // Per dimension: the grid points around `x` and their weights, 2 for
        // linear and 4 for the spline.
        let mut taps = Vec::with_capacity(f.input.len());
        let mut counts = Vec::with_capacity(f.input.len());
        for (dim, input) in f.input.iter().enumerate() {
            let (i, s) = input.position(x[dim]);
            let mut points = [(0usize, 0.0f32); 4];
            let count = match f.order {
                Interpolation::Linear => {
                    points[0] = (i, 1.0 - s);
                    points[1] = ((i + 1).min(input.size - 1), s);
                    2
                }
                Interpolation::Cubic => {
                    for (k, w) in cubic_weights(s).iter().enumerate() {
                        let j = (i as isize + k as isize - 1).clamp(0, input.size as isize - 1);
                        points[k] = (j as usize, *w);
                    }
                    4
                }
            };
            taps.push(points);
            counts.push(count);
        }

        out.fill(0.0);
        let combinations: usize = counts.iter().product();
        for combination in 0..combinations {
            let mut rest = combination;
            let mut weight = 1.0f32;
            let mut base = 0usize;
            for dim in 0..taps.len() {
                let k = rest % counts[dim];
                rest /= counts[dim];
                let (i, w) = taps[dim][k];
                weight *= w;
                base = base.saturating_add(i.saturating_mul(f.strides[dim]));
            }
            if weight == 0.0 {
                continue;
            }
            let start = base.saturating_mul(f.output_dim);
            if let Some(part) = f.samples.get(start..).and_then(|s| s.get(..f.output_dim)) {
                for (o, &v) in out.iter_mut().zip(part) {
                    *o += weight * v;
                }
            }
        }
        Ok(())
    }
}
