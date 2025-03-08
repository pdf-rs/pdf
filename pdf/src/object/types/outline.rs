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

#[derive(Clone, Debug, DataSize)]
pub enum Action {
    Goto(MaybeNamedDest),
    Other(Dictionary),
}
impl Object for Action {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut d = t!(p.resolve(resolve)?.into_dictionary());
        let s = try_opt!(d.get("S")).as_name()?;
        match s {
            "GoTo" => {
                let dest = t!(MaybeNamedDest::from_primitive(
                    try_opt!(d.remove("D")),
                    resolve
                ));
                Ok(Action::Goto(dest))
            }
            _ => Ok(Action::Other(d)),
        }
    }
}
impl ObjectWrite for Action {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            Action::Goto(dest) => {
                let mut dict = Dictionary::new();
                dict.insert("D", dest.to_primitive(update)?);
                Ok(Primitive::Dictionary(dict))
            }
            Action::Other(dict) => Ok(Primitive::Dictionary(dict.clone())),
        }
    }
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type = "Outlines?")]
pub struct Outlines {
    #[pdf(key = "Count", default = "0")]
    pub count: i32,

    #[pdf(key = "First")]
    pub first: Option<Ref<OutlineItem>>,

    #[pdf(key = "Last")]
    pub last: Option<Ref<OutlineItem>>,
}
