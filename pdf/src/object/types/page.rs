use super::prelude::*;

/*
use std::iter::once;
use itertools::Either;
// needs recursive types
impl PagesNode {
    pub fn pages<'a>(&'a self, resolve: &'a impl Resolve) -> impl Iterator<Item=Result<PageRc>> + 'a {
        match self {
            PagesNode::Tree(ref tree) => Either::Left(Box::new(tree.pages(resolve))),
            PagesNode::Leaf(ref page) => Either::Right(once(Ok(PageRc(page.clone()))))
        }
    }
}
*/

/// A `PagesNode::Leaf` wrapped in a `RcRef`
///
#[derive(Debug, Clone, DataSize)]
pub struct PageRc(pub(crate) RcRef<PagesNode>);
impl Deref for PageRc {
    type Target = Page;
    fn deref(&self) -> &Page {
        match *self.0 {
            PagesNode::Leaf(ref page) => page,
            _ => unreachable!(),
        }
    }
}
impl PageRc {
    pub fn create(page: Page, update: &mut impl Updater) -> Result<PageRc> {
        Ok(PageRc(update.create(PagesNode::Leaf(page))?))
    }
    pub fn update(page: Page, old_page: &PageRc, update: &mut impl Updater) -> Result<PageRc> {
        update
            .update(old_page.get_ref(), PagesNode::Leaf(page))
            .map(PageRc)
    }
    pub fn get_ref(&self) -> PlainRef {
        self.0.inner
    }
}
impl Object for PageRc {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<PageRc> {
        let node = t!(RcRef::from_primitive(p, resolve));
        match *node {
            PagesNode::Tree(_) => Err(PdfError::WrongDictionaryType {
                expected: "Page".into(),
                found: "Pages".into(),
            }),
            PagesNode::Leaf(_) => Ok(PageRc(node)),
        }
    }
}
impl ObjectWrite for PageRc {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.0.to_primitive(update)
    }
}

/// A `PagesNode::Tree` wrapped in a `RcRef`
///
#[derive(Debug, Clone, DataSize)]
pub struct PagesRc(RcRef<PagesNode>);
impl Deref for PagesRc {
    type Target = PageTree;
    fn deref(&self) -> &PageTree {
        match *self.0 {
            PagesNode::Tree(ref tree) => tree,
            _ => unreachable!(),
        }
    }
}
impl PagesRc {
    pub fn create(tree: PageTree, update: &mut impl Updater) -> Result<PagesRc> {
        Ok(PagesRc(update.create(PagesNode::Tree(tree))?))
    }
}
impl Object for PagesRc {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<PagesRc> {
        let node = t!(RcRef::from_primitive(p, resolve));
        match *node {
            PagesNode::Leaf(_) => Err(PdfError::WrongDictionaryType {
                expected: "Pages".into(),
                found: "Page".into(),
            }),
            PagesNode::Tree(_) => Ok(PagesRc(node)),
        }
    }
}
impl ObjectWrite for PagesRc {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.0.to_primitive(update)
    }
}

#[derive(Object, ObjectWrite, Debug, Clone, DataSize)]
#[pdf(Type = "Page?")]
pub struct Page {
    #[pdf(key = "Parent")]
    pub parent: PagesRc,

    #[pdf(key = "Resources", indirect)]
    pub resources: Option<MaybeRef<Resources>>,

    #[pdf(key = "MediaBox")]
    pub media_box: Option<Rectangle>,

    #[pdf(key = "CropBox")]
    pub crop_box: Option<Rectangle>,

    #[pdf(key = "TrimBox")]
    pub trim_box: Option<Rectangle>,

    #[pdf(key = "Contents")]
    pub contents: Option<Content>,

    #[pdf(key = "Rotate", default = "0")]
    pub rotate: i32,

    #[pdf(key = "Metadata")]
    pub metadata: Option<Primitive>,

    #[pdf(key = "LGIDict")]
    pub lgi: Option<Primitive>,

    #[pdf(key = "VP")]
    pub vp: Option<Primitive>,

    #[pdf(key = "Annots")]
    pub annotations: Lazy<Vec<MaybeRef<Annot>>>,

    #[pdf(other)]
    pub other: Dictionary,
}
fn inherit<'a, T: 'a, F>(mut parent: &'a PageTree, f: F) -> Result<Option<T>>
where
    F: Fn(&'a PageTree) -> Option<T>,
{
    loop {
        match (&parent.parent, f(parent)) {
            (_, Some(t)) => return Ok(Some(t)),
            (Some(ref p), None) => parent = p,
            (None, None) => return Ok(None),
        }
    }
}

impl Page {
    pub fn new(parent: PagesRc) -> Page {
        Page {
            parent,
            media_box: None,
            crop_box: None,
            trim_box: None,
            resources: None,
            contents: None,
            rotate: 0,
            metadata: None,
            lgi: None,
            vp: None,
            other: Dictionary::new(),
            annotations: Default::default(),
        }
    }
    pub fn media_box(&self) -> Result<Rectangle> {
        match self.media_box {
            Some(b) => Ok(b),
            None => {
                inherit(&self.parent, |pt| pt.media_box)?.ok_or_else(|| PdfError::MissingEntry {
                    typ: "Page",
                    field: "MediaBox".into(),
                })
            }
        }
    }
    pub fn crop_box(&self) -> Result<Rectangle> {
        match self.crop_box {
            Some(b) => Ok(b),
            None => match inherit(&self.parent, |pt| pt.crop_box)? {
                Some(b) => Ok(b),
                None => self.media_box(),
            },
        }
    }
    pub fn resources(&self) -> Result<&MaybeRef<Resources>> {
        match self.resources {
            Some(ref r) => Ok(r),
            None => inherit(&self.parent, |pt| pt.resources.as_ref())?.ok_or_else(|| {
                PdfError::MissingEntry {
                    typ: "Page",
                    field: "Resources".into(),
                }
            }),
        }
    }
}
impl SubType<PagesNode> for Page {}

#[derive(Object, DataSize, Debug, ObjectWrite)]
pub struct PageLabel {
    #[pdf(key = "S")]
    pub style: Option<Counter>,

    #[pdf(key = "P")]
    pub prefix: Option<PdfString>,

    #[pdf(key = "St")]
    pub start: Option<usize>,
}
