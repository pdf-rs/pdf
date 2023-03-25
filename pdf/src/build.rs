use crate::object::*;
use crate::content::*;
use crate::error::Result;
use crate::primitive::Primitive;

#[derive(Default)]
pub struct PageBuilder {
    pub content: Option<Content>,
    pub media_box: Option<Rect>,
    pub crop_box: Option<Rect>,
    pub trim_box: Option<Rect>,
    pub resources: Option<MaybeRef<Resources>>,
    pub rotate: i32,
    pub metadata: Option<Primitive>,
    pub lgi: Option<Primitive>,
    pub vp: Option<Primitive>,
}
impl PageBuilder {
    pub fn from_content(content: Content) -> PageBuilder {
        PageBuilder {
            content: Some(content),
            .. PageBuilder::default()
        }
    }
    pub fn from_page(page: &Page) -> Result<PageBuilder> {
        Ok(PageBuilder {
            content: page.contents.clone(),
            media_box: Some(page.media_box()?),
            crop_box: Some(page.crop_box()?),
            trim_box: page.trim_box,
            resources: Some(page.resources()?.clone()),
            rotate: page.rotate,
            metadata: page.metadata.clone(),
            lgi: page.lgi.clone(),
            vp: page.vp.clone(),
        })
    }
    pub fn size(&mut self, width: f32, height: f32) {
        self.media_box = Some(Rect {
            top: 0.,
            left: 0.,
            bottom: height,
            right: width,
        });
    }
    pub fn resources(&mut self, resources: MaybeRef<Resources>) {
        self.resources = Some(resources);
    }
}

pub struct CatalogBuilder {
    pages: Vec<PageBuilder>
}
impl CatalogBuilder {
    pub fn from_pages(pages: Vec<PageBuilder>) -> CatalogBuilder {
        CatalogBuilder {
            pages
        }
    }
    pub fn build(self, update: &mut impl Updater) -> Result<Catalog> {
        let kids_promise: Vec<_> = self.pages.iter()
            .map(|_page| update.promise::<PagesNode>())
            .collect();
        let kids: Vec<_> = kids_promise.iter()
            .map(|p| Ref::new(p.get_inner()))
            .collect();

        let tree = PagesRc::create(PageTree {
            parent: None,
            count: kids.len() as _,
            kids,
            resources: None,
            media_box: None,
            crop_box: None
        }, update)?;

        for (page, promise) in self.pages.into_iter().zip(kids_promise) {
            let page = Page {
                parent: tree.clone(),
                contents: page.content,
                media_box: page.media_box,
                crop_box: page.crop_box,
                trim_box: page.trim_box,
                resources: page.resources,
                rotate: page.rotate,
                metadata: page.metadata,
                lgi: page.lgi,
                vp: page.vp,
            };
            update.fulfill(promise, PagesNode::Leaf(page))?;
        }

        Ok(Catalog {
            pages: tree,
            names: None,
            dests: None,
            metadata: None,
            outlines: None,
            struct_tree_root: None,
            forms: None,
        })
    }
}