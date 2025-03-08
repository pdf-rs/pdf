use super::prelude::*;

#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type = "StructTreeRoot")]
pub struct StructTreeRoot {
    #[pdf(key = "K")]
    pub children: Vec<StructElem>,
}
#[derive(Object, ObjectWrite, Debug, DataSize)]
pub struct StructElem {
    #[pdf(key = "S")]
    pub struct_type: StructType,

    #[pdf(key = "P")]
    pub parent: Ref<StructElem>,

    #[pdf(key = "ID")]
    pub id: Option<PdfString>,

    /// `Pg`: A page object representing a page on which some or all of the content items designated by the K entry are rendered.
    #[pdf(key = "Pg")]
    pub page: Option<Ref<Page>>,
}

#[derive(Object, ObjectWrite, Debug, DataSize)]
pub enum StructType {
    Document,
    Part,
    Art,
    Sect,
    Div,
    BlockQuote,
    Caption,
    TOC,
    TOCI,
    Index,
    NonStruct,
    Private,
    Book,
    P,
    H,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    L,
    Ll,
    Lbl,
    LBody,
    Table,
    TR,
    TH,
    TD,
    THead,
    TBody,
    TFoot,
    Span,
    Quote,
    Note,
    Reference,
    BibEntry,
    Code,
    Link,
    Annot,
    Ruby,
    RB,
    RT,
    RP,
    Warichu,
    WT,
    WP,
    Figure,
    Formula,
    Form,
    #[pdf(other)]
    Other(String),
}
