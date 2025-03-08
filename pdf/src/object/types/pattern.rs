use super::prelude::*;

#[derive(Debug, Object, ObjectWrite, DataSize, Clone, DeepClone)]
pub struct PatternDict {
    #[pdf(key = "PaintType")]
    pub paint_type: Option<i32>,

    #[pdf(key = "TilingType")]
    pub tiling_type: Option<i32>,

    #[pdf(key = "BBox")]
    pub bbox: Rectangle,

    #[pdf(key = "XStep")]
    pub x_step: f32,

    #[pdf(key = "YStep")]
    pub y_step: f32,

    #[pdf(key = "Resources")]
    pub resources: Ref<Resources>,

    #[pdf(key = "Matrix")]
    pub matrix: Option<Matrix>,
}

#[derive(Debug, DataSize)]
pub enum Pattern {
    Dict(PatternDict),
    Stream(PatternDict, Vec<Op>),
}
impl Pattern {
    pub fn dict(&self) -> &PatternDict {
        match *self {
            Pattern::Dict(ref d) => d,
            Pattern::Stream(ref d, _) => d,
        }
    }
}
impl Object for Pattern {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let p = p.resolve(resolve)?;
        match p {
            Primitive::Dictionary(dict) => {
                Ok(Pattern::Dict(PatternDict::from_dict(dict, resolve)?))
            }
            Primitive::Stream(s) => {
                let stream: Stream<PatternDict> = Stream::from_stream(s, resolve)?;
                let data = stream.data(resolve)?;
                let ops = t!(parse_ops(&data, resolve));
                let dict = stream.info.info;
                Ok(Pattern::Stream(dict, ops))
            }
            p => Err(PdfError::UnexpectedPrimitive {
                expected: "Dictionary or Stream",
                found: p.get_debug_name(),
            }),
        }
    }
}
impl ObjectWrite for Pattern {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            Pattern::Dict(ref d) => d.to_primitive(update),
            Pattern::Stream(ref d, ref ops) => {
                let data = serialize_ops(ops)?;
                let stream = Stream::new_with_filters(d.clone(), data, vec![]);
                stream.to_primitive(update)
            }
        }
    }
}
impl DeepClone for Pattern {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        match *self {
            Pattern::Dict(ref d) => Ok(Pattern::Dict(d.deep_clone(cloner)?)),
            Pattern::Stream(ref dict, ref ops) => {
                let old_resources = cloner.get(dict.resources)?;
                let mut resources = Resources::default();
                let ops: Vec<Op> = ops
                    .iter()
                    .map(|op| deep_clone_op(op, cloner, &old_resources, &mut resources))
                    .collect::<Result<Vec<_>>>()?;
                let dict = PatternDict {
                    resources: cloner.create(resources)?.get_ref(),
                    ..*dict
                };
                Ok(Pattern::Stream(dict, ops))
            }
        }
    }
}
