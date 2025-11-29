use super::prelude::*;

#[derive(Object, Debug, Clone, DataSize)]
pub struct OutlineItem {
    #[pdf(key = "Title")]
    pub title: Option<PdfString>,

    #[pdf(key = "Prev")]
    pub prev: Option<Ref<OutlineItem>>,

    #[pdf(key = "Next")]
    pub next: Option<Ref<OutlineItem>>,

    #[pdf(key = "First")]
    pub first: Option<Ref<OutlineItem>>,

    #[pdf(key = "Last")]
    pub last: Option<Ref<OutlineItem>>,

    #[pdf(key = "Count", default = "0")]
    pub count: i32,

    #[pdf(key = "Dest")]
    pub dest: Option<Primitive>,

    #[pdf(key = "A")]
    pub action: Option<Action>,

    #[pdf(key = "SE")]
    pub se: Option<Dictionary>,

    #[pdf(key = "C")]
    pub color: Option<Vec<f32>>,

    #[pdf(key = "F")]
    pub flags: Option<i32>,
}

#[derive(Object, ObjectWrite, Clone, Debug, DataSize)]
#[pdf(Type = "Outlines?")]
pub struct Outlines {
    #[pdf(key = "Count", default = "0")]
    pub count: i32,

    #[pdf(key = "First")]
    pub first: Option<Ref<OutlineItem>>,

    #[pdf(key = "Last")]
    pub last: Option<Ref<OutlineItem>>,
}
