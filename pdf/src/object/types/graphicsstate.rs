use super::prelude::*;

#[derive(Object, ObjectWrite, DeepClone, Debug, DataSize, Copy, Clone)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}
#[derive(Object, ObjectWrite, DeepClone, Debug, DataSize, Copy, Clone)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

#[derive(Object, ObjectWrite, DeepClone, Debug, DataSize, Clone)]
#[pdf(Type = "ExtGState?")]
/// `ExtGState`
pub struct GraphicsStateParameters {
    #[pdf(key = "LW")]
    pub line_width: Option<f32>,

    #[pdf(key = "LC")]
    pub line_cap: Option<LineCap>,

    #[pdf(key = "LJ")]
    pub line_join: Option<LineJoin>,

    #[pdf(key = "ML")]
    pub miter_limit: Option<f32>,

    #[pdf(key = "D")]
    pub dash_pattern: Option<Vec<Primitive>>,

    #[pdf(key = "RI")]
    pub rendering_intent: Option<Name>,

    #[pdf(key = "OP")]
    pub overprint: Option<bool>,

    #[pdf(key = "op")]
    pub overprint_fill: Option<bool>,

    #[pdf(key = "OPM")]
    pub overprint_mode: Option<i32>,

    #[pdf(key = "Font")]
    pub font: Option<(Ref<Font>, f32)>,

    // BG
    // BG2
    // UCR
    // UCR2
    // TR
    // TR2
    // HT
    // FL
    // SM
    // SA
    #[pdf(key = "BM")]
    pub blend_mode: Option<Primitive>,

    #[pdf(key = "SMask")]
    pub smask: Option<Primitive>,

    #[pdf(key = "CA")]
    pub stroke_alpha: Option<f32>,

    #[pdf(key = "ca")]
    pub fill_alpha: Option<f32>,

    #[pdf(key = "AIS")]
    pub alpha_is_shape: Option<bool>,

    #[pdf(key = "TK")]
    pub text_knockout: Option<bool>,

    #[pdf(other)]
    _other: Dictionary,
}
