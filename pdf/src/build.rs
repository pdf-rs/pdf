use crate::object::*;
use crate::content::*;
use crate::error::Result;

#[derive(Default)]
pub struct PageBuilder {
    content: Option<Content>,
    media_box: Option<Rect>,
    crop_box: Option<Rect>,
    trim_box: Option<Rect>,
    resources: Option<Resources>,
}
impl PageBuilder {
    pub fn from_content(content: Content) -> PageBuilder {
        PageBuilder {
            content: Some(content),
            .. PageBuilder::default()
        }
    }
    pub fn size(&mut self, width: f32, height: f32) {
        self.media_box = Some(Rect {
            top: 0.,
            left: 0.,
            bottom: height,
            right: width,
        });
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
        let kids_promise: Vec<_> = self.pages.iter().map(|page| update.promise::<Page>()).collect();
        let kids: Vec<_> = kids_promise.iter().map(|p| p.get_ref().upcast()).collect();

        let tree = update.create(PageTree {
            parent: None,
            count: kids.len() as _,
            kids,
            resources: None,
            media_box: None,
            crop_box: None
        })?;

        for (page, promise) in self.pages.into_iter().zip(kids_promise) {
            let page = Page {
                parent: tree.clone(),
                contents: page.content,
                media_box: page.media_box,
                crop_box: page.crop_box,
                trim_box: page.trim_box,
                resources: None,
            };
            update.fulfill(promise, page)?;
        }

        Ok(Catalog {
            pages: tree.into(),
            names: None,
            dests: None,
            metadata: None,
            outlines: None,
            struct_tree_root: None
        })
    }
}