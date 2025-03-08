use super::page::Page;
use super::prelude::*;

/// Node in a page tree - type is either `Page` or `PageTree`
#[derive(Debug, Clone, DataSize)]
pub enum PagesNode {
    Tree(PageTree),
    Leaf(Page),
}

impl Object for PagesNode {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<PagesNode> {
        let mut dict = p.resolve(resolve)?.into_dictionary()?;
        match dict.require("PagesNode", "Type")?.as_name()? {
            "Page" => Ok(PagesNode::Leaf(t!(Page::from_dict(dict, resolve)))),
            "Pages" => Ok(PagesNode::Tree(t!(PageTree::from_dict(dict, resolve)))),
            other => Err(PdfError::WrongDictionaryType {
                expected: "Page or Pages".into(),
                found: other.into(),
            }),
        }
    }
}
impl ObjectWrite for PagesNode {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match *self {
            PagesNode::Tree(ref t) => t.to_primitive(update),
            PagesNode::Leaf(ref l) => l.to_primitive(update),
        }
    }
}

#[derive(Object, ObjectWrite, Debug, Default, Clone, DataSize)]
#[pdf(Type = "Pages?")]
pub struct PageTree {
    #[pdf(key = "Parent")]
    pub parent: Option<PagesRc>,

    #[pdf(key = "Kids")]
    pub kids: Vec<Ref<PagesNode>>,

    #[pdf(key = "Count")]
    pub count: u32,

    #[pdf(key = "Resources")]
    pub resources: Option<MaybeRef<Resources>>,

    #[pdf(key = "MediaBox")]
    pub media_box: Option<Rectangle>,

    #[pdf(key = "CropBox")]
    pub crop_box: Option<Rectangle>,
}
impl PageTree {
    pub fn page(&self, resolve: &impl Resolve, page_nr: u32) -> Result<PageRc> {
        self.page_limited(resolve, page_nr, 16)
    }
    fn page_limited(&self, resolve: &impl Resolve, page_nr: u32, depth: usize) -> Result<PageRc> {
        if depth == 0 {
            bail!("page tree depth exeeded");
        }
        let mut pos = 0;
        for &kid in &self.kids {
            let node = resolve.get(kid)?;
            match *node {
                PagesNode::Tree(ref tree) => {
                    if (pos..pos + tree.count).contains(&page_nr) {
                        return tree.page_limited(resolve, page_nr - pos, depth - 1);
                    }
                    pos += tree.count;
                }
                PagesNode::Leaf(ref _page) => {
                    if pos == page_nr {
                        return Ok(PageRc(node));
                    }
                    pos += 1;
                }
            }
        }
        Err(PdfError::PageOutOfBounds { page_nr, max: pos })
    }

    /*
    pub fn update_pages(&mut self, mut offset: u32, page_nr: u32, page: Page) -> Result<()> {
        for kid in &self.kids {
            // println!("{}/{} {:?}", offset, page_nr, kid);
            match *(self.get(*kid)?) {
                PagesNode::Tree(ref mut t) => {
                    if offset + t.count < page_nr {
                        offset += t.count;
                    } else {
                        return self.update_pages(t, offset, page_nr, page);
                    }
                },
                PagesNode::Leaf(ref mut p) => {
                    if offset < page_nr {
                        offset += 1;
                    } else {
                        assert_eq!(offset, page_nr);
                        let p = self.storage.create(page)?;
                        self.storage.update(kid.get_inner(), PagesNode::Leaf(p));
                        return Ok(());
                    }
                }
            }

        }
        Err(PdfError::PageNotFound {page_nr: page_nr})
    }
    pub fn pages<'a>(&'a self, resolve: &'a impl Resolve) -> impl Iterator<Item=Result<PageRc>> + 'a {
        self.kids.iter().flat_map(move |&r| {
            match resolve.get(r) {
                Ok(node) => Either::Left(node.pages(resolve)),
                Err(e) => Either::Right(once(Err(e)))
            }
        })
    }
    */
}
impl SubType<PagesNode> for PageTree {}
