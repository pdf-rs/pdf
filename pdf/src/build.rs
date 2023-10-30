use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use datasize::DataSize;

use crate::PdfError;
use crate::any::AnySync;
use crate::enc::StreamFilter;
use crate::file::Cache;
use crate::file::FileOptions;
use crate::file::Log;
use crate::file::Storage;
use crate::file::Trailer;
use crate::object::*;
use crate::content::*;
use crate::error::Result;
use crate::parser::ParseFlags;
use crate::primitive::Dictionary;
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
    pub other: Dictionary,
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
            other: page.other.clone(),
        })
    }
    pub fn clone_page(page: &Page, cloner: &mut impl Cloner) -> Result<PageBuilder> {
        let old_resources = &**page.resources()?.data();

        let mut resources = Resources::default();
        let content = page.contents.as_ref()
            .map(|content| content.operations(cloner)).transpose()?
            .map(|ops| {
                ops.into_iter().map(|op| -> Result<Op, PdfError> {
                    deep_clone_op(&op, cloner, old_resources, &mut resources)
                }).collect()
            })
            .transpose()?
            .map(|ops| Content::from_ops(ops));

        Ok(PageBuilder {
            content,
            media_box: Some(page.media_box()?),
            crop_box: Some(page.crop_box()?),
            trim_box: page.trim_box,
            resources: Some(cloner.create(resources)?.into()),
            rotate: page.rotate,
            metadata: page.metadata.deep_clone(cloner)?,
            lgi: page.lgi.deep_clone(cloner)?,
            vp: page.vp.deep_clone(cloner)?,
            other: page.other.deep_clone(cloner)?,
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
                other: page.other,
            };
            update.fulfill(promise, PagesNode::Leaf(page))?;
        }

        Ok(Catalog {
            version: Some("1.7".into()),
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

pub struct PdfBuilder<SC, OC, L> {
    pub storage: Storage<Vec<u8>, SC, OC, L>,
    pub info: Option<InfoDict>,
    pub id: Option<[String; 2]>,

}
impl<SC, OC, L> PdfBuilder<SC, OC, L>
where
    SC: Cache<Result<AnySync, Arc<PdfError>>>,
    OC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    pub fn new(fileoptions: FileOptions<'_, SC, OC, L>) -> Self {
        let storage = fileoptions.storage();
        PdfBuilder {
            storage,
            info: None,
            id: None
        }
    }
    pub fn info(mut self, info: InfoDict) -> Self {
        self.info = Some(info);
        self
    }
    pub fn id(mut self, a: String, b: String) -> Self {
        self.id = Some([a, b]);
        self
    }
    pub fn build(mut self, catalog: CatalogBuilder) -> Result<Vec<u8>> {
        let catalog = catalog.build(&mut self.storage)?;
        
        let mut trailer = Trailer {
            root: self.storage.create(catalog)?,
            encrypt_dict: None,
            size: 0,
            id: vec!["foo".into(), "bar".into()],
            info_dict: self.info,
            prev_trailer_pos: None,
        };
        self.storage.save(&mut trailer)?;
        Ok(self.storage.into_inner())
    }
}
pub struct Importer<'a, R, U> {
    resolver: R,
    map: HashMap<PlainRef, PlainRef>,
    updater: &'a mut U,
    rcrefs: HashMap<PlainRef, AnySync>,
    // ptr of old -> (old, new)
    shared: HashMap<usize, (AnySync, AnySync)>,
}


impl<'a, R, U> Importer<'a, R, U> {
    pub fn new(resolver: R, updater: &'a mut U) -> Self {
        Importer {
            resolver,
            updater,
            map: Default::default(),
            rcrefs: Default::default(),
            shared: Default::default()
        }
    }
}
impl<'a, R: Resolve, U> Resolve for Importer<'a, R, U> {
    fn get<T: Object+datasize::DataSize>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        self.resolver.get(r)
    }
    fn get_data_or_decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>> {
        self.resolver.get_data_or_decode(id, range, filters)
    }
    fn options(&self) -> &ParseOptions {
        self.resolver.options()
    }
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        self.resolver.resolve(r)
    }
    fn resolve_flags(&self, r: PlainRef, flags: ParseFlags, depth: usize) -> Result<Primitive> {
        self.resolver.resolve_flags(r, flags, depth)
    }
    fn stream_data(&self, id: PlainRef, range: Range<usize>) -> Result<Arc<[u8]>> {
        self.resolver.stream_data(id, range)
    }
}
impl<'a, R, U: Updater> Updater for Importer<'a, R, U> {
    fn create<T: ObjectWrite>(&mut self, obj: T) -> Result<RcRef<T>> {
        self.updater.create(obj)
    }
    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>> {
        self.updater.fulfill(promise, obj)
    }
    fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        self.updater.promise()
    }
    fn update<T: ObjectWrite>(&mut self, old: PlainRef, obj: T) -> Result<RcRef<T>> {
        self.updater.update(old, obj)
    }
}
impl<'a, R: Resolve, U: Updater> Cloner for Importer<'a, R, U> {
    fn clone_ref<T: DeepClone + Object + DataSize + ObjectWrite>(&mut self, old: Ref<T>) -> Result<Ref<T>> {
        if let Some(&new_ref) = self.map.get(&old.get_inner()) {
            return Ok(Ref::new(new_ref));
        }
        let obj = self.resolver.get(old)?;
        let clone = obj.deep_clone(self)?;

        let r = self.updater.create(clone)?;
        self.map.insert(old.get_inner(), r.get_ref().get_inner());
        Ok(r.get_ref())
    }
    fn clone_plainref(&mut self, old: PlainRef) -> Result<PlainRef> {
        if let Some(&new_ref) = self.map.get(&old) {
            return Ok(new_ref);
        }
        let obj = self.resolver.resolve(old)?;
        let clone = obj.deep_clone(self)?;

        let new = self.updater.create(clone)?
            .get_ref().get_inner();

        self.map.insert(old, new);
        Ok(new)
    }
    fn clone_rcref<T: DeepClone + ObjectWrite + DataSize>(&mut self, old: &RcRef<T>) -> Result<RcRef<T>> {
        let old_ref = old.get_ref().get_inner();
        if let Some(&new_ref) = self.map.get(&old_ref) {
            let arc = self.rcrefs.get(&new_ref).unwrap().clone().downcast()?;
            return Ok(RcRef::new(new_ref, arc));
        }

        let new = old.data().deep_clone(self)?;
        let new = self.updater.create::<T>(new)?;
        self.rcrefs.insert(new.get_ref().get_inner(), AnySync::new(new.data().clone()));
        self.map.insert(old_ref, new.get_ref().get_inner());
        Ok(new)
    }
    fn clone_shared<T: DeepClone>(&mut self, old: &Shared<T>) -> Result<Shared<T>> {
        let key = &**old as *const T as usize;
        if let Some((old, new)) = self.shared.get(&key) {
            return new.clone().downcast();
        }
        let new = Shared::new(old.as_ref().deep_clone(self)?);
        self.shared.insert(key, (AnySync::new_without_size(old.clone()), AnySync::new_without_size(new.clone())));
        Ok(new)
    }
}