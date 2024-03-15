use std::collections::HashMap;
use std::collections::HashSet;
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
    pub ops: Vec<Op>,
    pub media_box: Option<Rectangle>,
    pub crop_box: Option<Rectangle>,
    pub trim_box: Option<Rectangle>,
    pub resources: Resources,
    pub rotate: i32,
    pub metadata: Option<Primitive>,
    pub lgi: Option<Primitive>,
    pub vp: Option<Primitive>,
    pub other: Dictionary,
}
impl PageBuilder {
    pub fn from_content(content: Content, resolve: &impl Resolve) -> Result<PageBuilder> {
        Ok(PageBuilder {
            ops: content.operations(resolve)?,
            .. PageBuilder::default()
        })
    }
    pub fn from_page(page: &Page, resolve: &impl Resolve) -> Result<PageBuilder> {
        Ok(PageBuilder {
            ops: page.contents.as_ref().map(|c| c.operations(resolve)).transpose()?.unwrap_or_default(),
            media_box: Some(page.media_box()?),
            crop_box: Some(page.crop_box()?),
            trim_box: page.trim_box,
            resources: (**page.resources()?.data()).clone(),
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
        let ops = page.contents.as_ref()
            .map(|content| content.operations(cloner)).transpose()?
            .map(|ops| {
                ops.into_iter().map(|op| -> Result<Op, PdfError> {
                    deep_clone_op(&op, cloner, old_resources, &mut resources)
                }).collect()
            })
            .transpose()?
            .unwrap_or_default();

        Ok(PageBuilder {
            ops,
            media_box: Some(page.media_box()?),
            crop_box: Some(page.crop_box()?),
            trim_box: page.trim_box,
            resources,
            rotate: page.rotate,
            metadata: page.metadata.deep_clone(cloner)?,
            lgi: page.lgi.deep_clone(cloner)?,
            vp: page.vp.deep_clone(cloner)?,
            other: page.other.deep_clone(cloner)?,
        })
    }
    pub fn size(&mut self, width: f32, height: f32) {
        self.media_box = Some(Rectangle {
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
            let content = Content::from_ops(page.ops);
            let resources = update.create(page.resources)?.into();
            let page = Page {
                parent: tree.clone(),
                contents: Some(content),
                media_box: page.media_box,
                crop_box: page.crop_box,
                trim_box: page.trim_box,
                resources: Some(resources),
                rotate: page.rotate,
                metadata: page.metadata,
                lgi: page.lgi,
                vp: page.vp,
                other: page.other,
                annotations: Default::default(),
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
            page_labels: None,
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

pub struct ImporterMap<R> {
    resolver: R,
    map: HashMap<PlainRef, PlainRef>,
}

impl<'a, R, U> Importer<'a, R, U> {
    pub fn new(resolver: R, updater: &'a mut U) -> Self {
        Importer {
            resolver,
            updater,
            map: Default::default(),
            rcrefs: Default::default(),
            shared: Default::default(),
        }
    }
}
impl<'a, R: Resolve, U> Importer<'a, R, U> {
    pub fn finish(self) -> ImporterMap<R> {
        ImporterMap { resolver: self.resolver, map: self.map }
    }
}
impl<R: Resolve> ImporterMap<R> {
    fn compare_dict(&self, a_dict: &Dictionary, b_dict: &Dictionary, new_resolve: &impl Resolve) -> Result<bool> {
        let mut same = true;
        let mut b_unvisited: HashSet<_> = b_dict.keys().collect();
        for (a_key, a_val) in a_dict.iter() {
            if let Some(b_val) = b_dict.get(a_key) {
                if !self.compare_prim(a_val, b_val, new_resolve)? {
                    println!("value for key {a_key} mismatch.");
                    same = false;
                }
                b_unvisited.remove(a_key);
            } else {
                println!("missing key {a_key} in b.");
                same = false;
            }
        }
        for b_key in b_unvisited.iter() {
            println!("missing key {b_key} in a.");
        }
        Ok(same && !b_unvisited.is_empty())
    }
    fn compare_prim(&self, a: &Primitive, b: &Primitive, new_resolve: &impl Resolve) -> Result<bool> {
        match (a, b) {
            (Primitive::Array(a_parts), Primitive::Array(b_parts)) => {
                if a_parts.len() != b_parts.len() {
                    dbg!(a_parts, b_parts);
                    println!("different length {} vs. {}", a_parts.len(), b_parts.len());
                    println!("a = {a_parts:?}");
                    println!("b = {b_parts:?}");
                    return Ok(false);
                }
                for (a, b) in a_parts.iter().zip(b_parts.iter()) {
                    if !self.compare_prim(a, b, new_resolve)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Primitive::Dictionary(a_dict), Primitive::Dictionary(b_dict)) => {
                self.compare_dict(a_dict, b_dict, new_resolve)
            }
            (Primitive::Reference(r1), Primitive::Reference(r2)) => {
                match self.map.get(&r1) {
                    Some(r) if r == r2 => Ok(true),
                    _ => Ok(false)
                }
            }
            (Primitive::Stream(a_s), Primitive::Stream(b_s)) => {
                if !self.compare_dict(&a_s.info, &b_s.info, new_resolve)? {
                    println!("stream dicts differ");
                    return Ok(false)
                }
                let a_data = a_s.raw_data(&self.resolver)?;
                let b_data = b_s.raw_data(new_resolve)?;
                if a_data != b_data {
                    println!("data differs.");
                    return Ok(false)
                }
                Ok(true)
            }
            (Primitive::Integer(a), Primitive::Number(b)) => Ok(*a as f32 == *b),
            (Primitive::Number(a), Primitive::Integer(b)) => Ok(*a == *b as f32),
            (Primitive::Reference(a_ref), b) => {
                let a = self.resolver.resolve(*a_ref)?;
                self.compare_prim(&a, b, new_resolve)
            }
            (a, Primitive::Reference(b_ref)) => {
                let b = new_resolve.resolve(*b_ref)?;
                self.compare_prim(a, &b, new_resolve)
            }
            (ref a, ref b) => {
                if a == b {
                    Ok(true)
                } else {
                    println!("{a:?} != {b:?}");
                    Ok(false)
                }
            }
        }
    }
    pub fn verify(&self, new_resolve: &impl Resolve) -> Result<bool> {
        let mut same = true;
        for (&old_ref, &new_ref) in self.map.iter() {
            let old = self.resolver.resolve(old_ref)?;
            let new = new_resolve.resolve(new_ref)?;

            if !self.compare_prim(&old, &new, new_resolve)? {
                same = false;
            }
        }
        Ok(same)
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