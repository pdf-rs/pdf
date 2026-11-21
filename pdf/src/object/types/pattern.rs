use super::prelude::*;

/// A pattern dictionary, §8.7.3.1.
///
/// `/PatternType` decides which entries are legal: a tiling pattern
/// (type 1) needs `/BBox`, `/XStep`, `/YStep` and `/Resources`, while a
/// shading pattern (type 2) needs `/Shading` and none of those. So the
/// tiling-only entries are `Option`s here: the type is not known until the
/// dictionary has been read, and a shading pattern used to fail to parse
/// (missing `/BBox`) — which lost the whole `/Pattern` resource with it.
#[derive(Debug, Object, ObjectWrite, DataSize, Clone, DeepClone)]
pub struct PatternDict {
    #[pdf(key = "PatternType")]
    pub pattern_type: Option<i32>,

    #[pdf(key = "PaintType")]
    pub paint_type: Option<i32>,

    #[pdf(key = "TilingType")]
    pub tiling_type: Option<i32>,

    /// tiling patterns: the cell box, in pattern space.
    #[pdf(key = "BBox")]
    pub bbox: Option<Rectangle>,

    #[pdf(key = "XStep")]
    pub x_step: Option<f32>,

    #[pdf(key = "YStep")]
    pub y_step: Option<f32>,

    /// tiling patterns: the resources their content stream uses.
    #[pdf(key = "Resources")]
    pub resources: Option<Ref<Resources>>,

    #[pdf(key = "Matrix")]
    pub matrix: Option<Matrix>,

    /// shading patterns (paint type 2): the shading to paint. Raw, as in
    /// `Resources::shading` — it may be a dictionary or a mesh stream.
    #[pdf(key = "Shading")]
    pub shading: Option<Primitive>,
}

impl PatternDict {
    /// `true` for a shading pattern (§8.7.3.5) — painted with `/Shading`
    /// instead of a tiling cell.
    pub fn is_shading(&self) -> bool {
        self.paint_type == Some(2) || (self.shading.is_some() && self.bbox.is_none())
    }
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
                let stream = Stream::new_with_filters(d.clone(), &data, vec![])?;
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
                // A shading pattern (paint type 2) has no cell and no cell
                // resources: its content ops are not reached, and there is
                // nothing to rewrite them against.
                let Some(res_ref) = dict.resources else {
                    return Ok(Pattern::Stream(dict.deep_clone(cloner)?, ops.clone()));
                };
                let old_resources = cloner.get(res_ref)?;
                let mut resources = Resources::default();
                let ops: Vec<Op> = ops
                    .iter()
                    .map(|op| deep_clone_op(op, cloner, &old_resources, &mut resources))
                    .collect::<Result<Vec<_>>>()?;
                let dict = PatternDict {
                    resources: Some(cloner.create(resources)?.get_ref()),
                    ..dict.clone()
                };
                Ok(Pattern::Stream(dict, ops))
            }
        }
    }
}
