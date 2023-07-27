//! This is kind of the entry-point of the type-safe PDF functionality.
use std::marker::PhantomData;
use std::collections::HashMap;
use std::sync::{Arc};
use std::path::Path;
use std::io::Write;

use crate as pdf;
use crate::error::*;
use crate::object::*;
use crate::primitive::{Primitive, Dictionary, PdfString};
use crate::backend::Backend;
use crate::any::*;
use crate::parser::{Lexer, parse_with_lexer};
use crate::parser::{parse_indirect_object, parse, ParseFlags};
use crate::xref::{XRef, XRefTable, XRefInfo};
use crate::crypt::Decoder;
use crate::crypt::CryptDict;
use crate::enc::{StreamFilter, decode};
use std::ops::Range;
use datasize::DataSize;

#[cfg(feature="cache")]
pub use globalcache::{ValueSize, sync::SyncCache};

#[must_use]
pub struct PromisedRef<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}
impl<T> PromisedRef<T> {
    pub fn get_inner(&self) -> PlainRef {
        self.inner
    }
    pub fn get_ref(&self) -> Ref<T> {
        Ref::new(self.inner)
    }
}

pub trait Cache<T: Clone> {
    fn get_or_compute(&self, key: PlainRef, compute: impl FnOnce() -> T) -> T;
}
pub struct NoCache;
impl<T: Clone> Cache<T> for NoCache {
    fn get_or_compute(&self, key: PlainRef, compute: impl FnOnce() -> T) -> T {
        compute()
    }
}

#[cfg(feature="cache")]
impl<T: Clone + ValueSize + Send + 'static> Cache<T> for Arc<SyncCache<PlainRef, T>> {
    fn get_or_compute(&self, key: PlainRef, compute: impl FnOnce() -> T) -> T {
        self.get(key, compute)
    }
}

pub trait Log {
    fn load_object(&self, r: PlainRef) {}
}
pub struct NoLog;
impl Log for NoLog {}

pub struct Storage<B, OC, SC, L> {
    // objects identical to those in the backend
    cache: OC,
    stream_cache: SC,

    // objects that differ from the backend
    changes:    HashMap<ObjNr, Primitive>,

    refs:       XRefTable,

    decoder:    Option<Decoder>,
    options:    ParseOptions,

    backend:    B,

    // Position of the PDF header in the file.
    start_offset: usize,

    log: L
}

impl<OC, SC, L> Storage<Vec<u8>, OC, SC, L>
where
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    pub fn empty(object_cache: OC, stream_cache: SC, log: L) -> Self {
        Storage {
            cache: object_cache,
            stream_cache,
            changes: HashMap::new(),
            refs: XRefTable::new(0),
            decoder: None,
            options: ParseOptions::strict(),
            backend: Vec::from(&b"%PDF-1.7\n"[..]),
            start_offset: 0,
            log
        }
    }
}

impl<B, OC, SC, L> Storage<B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    pub fn into_inner(self) -> B {
        self.backend
    }
    pub fn with_cache(backend: B, options: ParseOptions, object_cache: OC, stream_cache: SC, log: L) -> Result<Self> {
        Ok(Storage {
            start_offset: backend.locate_start_offset()?,
            backend,
            refs: XRefTable::new(0),
            cache: object_cache,
            stream_cache,
            changes: HashMap::new(),
            decoder: None,
            options,
            log
        })
    }
    fn decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>> {
        let data = self.backend.read(range)?;

        let mut data = Vec::from(data);
        if let Some(ref decoder) = self.decoder {
            data = Vec::from(t!(decoder.decrypt(id, &mut data)));
        }
        for filter in filters {
            data = t!(decode(&data, filter), filter);
        }
        Ok(data.into())
    }

    pub fn load_storage_and_trailer(&mut self) -> Result<Dictionary> {
        self.load_storage_and_trailer_password(b"")
    }

    pub fn load_storage_and_trailer_password(&mut self, password: &[u8]) -> Result<Dictionary> {
        let (refs, trailer) = t!(self.backend.read_xref_table_and_trailer(self.start_offset, self));
        self.refs = refs;

        if let Some(crypt) = trailer.get("Encrypt") {
            let key = trailer
                .get("ID")
                .ok_or(PdfError::MissingEntry {
                    typ: "Trailer",
                    field: "ID".into(),
                })?
                .as_array()?[0]
                .as_string()?
                .as_bytes();
            let dict = CryptDict::from_primitive(crypt.clone(), self)?;
            self.decoder = Some(t!(Decoder::from_password(&dict, key, password)));
            if let Primitive::Reference(reference) = crypt {
                self.decoder.as_mut().unwrap().encrypt_indirect_object = Some(*reference);
            }
            if let Some(Primitive::Reference(catalog_ref)) = trailer.get("Root") {
                let catalog = t!(t!(self.resolve(*catalog_ref)).resolve(self)?.into_dictionary());
                if let Some(Primitive::Reference(metadata_ref)) = catalog.get("Metadata") {
                    self.decoder.as_mut().unwrap().metadata_indirect_object = Some(*metadata_ref);
                }
            }
        }
        Ok(trailer)
    }
    pub fn scan(&self) -> impl Iterator<Item = Result<ScanItem>> + '_ {
        let xref_offset = self.backend.locate_xref_offset().unwrap();
        let slice = self.backend.read(self.start_offset .. xref_offset).unwrap();
        let mut lexer = Lexer::with_offset(slice, 0);
        
        fn skip_xref(lexer: &mut Lexer) -> Result<()> {
            while lexer.next()? != "trailer" {

            }
            Ok(())
        }

        std::iter::from_fn(move || {
            loop {
                let pos = lexer.get_pos();
                match parse_indirect_object(&mut lexer, self, self.decoder.as_ref(), ParseFlags::all()) {
                    Ok((r, p)) => return Some(Ok(ScanItem::Object(r, p))),
                    Err(e) if e.is_eof() => return None,
                    Err(e) => {
                        lexer.set_pos(pos);
                        if let Ok(s) = lexer.next() {
                            debug!("next: {:?}", String::from_utf8_lossy(s.as_slice()));
                            if s == "xref" {
                                if let Err(e) = skip_xref(&mut lexer) {
                                    return Some(Err(e));
                                }
                                if let Ok(trailer) = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::DICT).and_then(|p| p.into_dictionary()) {
                                    return Some(Ok(ScanItem::Trailer(trailer)));
                                }
                            }
                            else if s == "startxref" {
                                if let Ok(offset) = lexer.next() {
                                    continue;
                                }
                            }
                        }
                        return Some(Err(e));
                    }
                }
            }
        })
    }
}

pub enum ScanItem {
    Object(PlainRef, Primitive),
    Trailer(Dictionary)
}

impl<B, OC, SC, L> Resolve for Storage<B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log
{
    fn resolve_flags(&self, r: PlainRef, flags: ParseFlags, depth: usize) -> Result<Primitive> {
        self.log.load_object(r);

        match self.changes.get(&r.id) {
            Some(p) => Ok((*p).clone()),
            None => match t!(self.refs.get(r.id)) {
                XRef::Raw {pos, ..} => {
                    let mut lexer = Lexer::with_offset(t!(self.backend.read(self.start_offset + pos ..)), self.start_offset + pos);
                    let p = t!(parse_indirect_object(&mut lexer, self, self.decoder.as_ref(), flags)).1;
                    Ok(p)
                }
                XRef::Stream {stream_id, index} => {
                    if !flags.contains(ParseFlags::STREAM) {
                        return Err(PdfError::PrimitiveNotAllowed { found: ParseFlags::STREAM, allowed: flags });
                    }
                    if depth == 0 {
                        bail!("too deep");
                    }
                    // use get to cache the object stream
                    let obj_stream = self.get::<ObjectStream>(Ref::from_id(stream_id))?;

                    let (data, range) = t!(obj_stream.get_object_slice(index, self));
                    let slice = data.get(range.clone()).ok_or_else(|| other!("invalid range {:?}, but only have {} bytes", range, data.len()))?;
                    parse(slice, self, flags)
                }
                XRef::Free {..} => err!(PdfError::FreeObject {obj_nr: r.id}),
                XRef::Promised => unimplemented!(),
                XRef::Invalid => err!(PdfError::NullRef {obj_nr: r.id}),
            }
        }
    }

    fn get<T: Object+DataSize>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        let key = r.get_inner();
        
        let res = self.cache.get_or_compute(key, || {
            match self.resolve(key).and_then(|p| T::from_primitive(p, self)) {
                Ok(obj) => Ok(Shared::new(obj).into()),
                Err(e) => Err(Arc::new(e)),
            }
        });
        match res {
            Ok(any) => Ok(RcRef::new(key, any.downcast()?)),
            Err(e) => Err(PdfError::Shared { source: e.clone()}),
        }
    }
    fn options(&self) -> &ParseOptions {
        &self.options
    }
    fn get_data_or_decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>> {
        self.stream_cache.get_or_compute(id, || self.decode(id, range, filters).map_err(Arc::new))
        .map_err(|e| e.into())
    }
}

impl<B, OC, SC, L> Updater for Storage<B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    fn create<T: ObjectWrite>(&mut self, obj: T) -> Result<RcRef<T>> {
        let id = self.refs.len() as u64;
        self.refs.push(XRef::Promised);
        let primitive = obj.to_primitive(self)?;
        self.changes.insert(id, primitive);
        let rc = Shared::new(obj);
        let r = PlainRef { id, gen: 0 };
        
        Ok(RcRef::new(r, rc))
    }
    fn update<T: ObjectWrite>(&mut self, old: PlainRef, obj: T) -> Result<RcRef<T>> {
        let r = match self.refs.get(old.id)? {
            XRef::Free { .. } => panic!(),
            XRef::Raw { gen_nr, .. } => PlainRef { id: old.id, gen: gen_nr + 1 },
            XRef::Stream { .. } => return self.create(obj),
            XRef::Promised => PlainRef { id: old.id, gen: 0 },
            XRef::Invalid => panic!()
        };
        let primitive = obj.to_primitive(self)?;
        self.changes.insert(old.id, primitive);
        let rc = Shared::new(obj);
        
        Ok(RcRef::new(r, rc))
    }

    fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        let id = self.refs.len() as u64;
        
        self.refs.push(XRef::Promised);
        
        PromisedRef {
            inner: PlainRef {
                id,
                gen: 0
            },
            _marker:    PhantomData
        }
    }
    
    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>> {
        self.update(promise.inner, obj)
    }
}

impl<OC, SC, L> Storage<Vec<u8>, OC, SC, L>
where
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log
{
    pub fn save(&mut self, trailer: &mut Trailer) -> Result<&[u8]> {
        // writing the trailer generates another id for the info dictionary
        trailer.size = (self.refs.len() + 2) as _;
        let trailer = trailer.to_dict(self)?;
        
        let xref_promise = self.promise::<Stream<XRefInfo>>();


        let mut changes: Vec<_> = self.changes.iter().collect();
        changes.sort_unstable_by_key(|&(id, _)| id);

        for (&id, primitive) in changes.iter() {
            let pos = self.backend.len();
            self.refs.set(id, XRef::Raw { pos: pos as _, gen_nr: 0 });
            writeln!(self.backend, "{} {} obj", id, 0)?;
            primitive.serialize(&mut self.backend)?;
            writeln!(self.backend, "endobj")?;
        }

        let xref_pos = self.backend.len();
        self.refs.set(xref_promise.get_inner().id, XRef::Raw { pos: xref_pos, gen_nr: 0 });
        // only write up to the xref stream obj id
        let stream = self.refs.write_stream(xref_promise.get_inner().id as usize + 1)?;

        writeln!(self.backend, "{} {} obj", xref_promise.get_inner().id, 0)?;
        let mut xref_and_trailer = stream.to_pdf_stream(&mut NoUpdate)?;
        for (k, v) in trailer.into_iter() {
            xref_and_trailer.info.insert(k, v);
        }

        xref_and_trailer.serialize(&mut self.backend)?;
        writeln!(self.backend, "endobj")?;

        let _ = self.fulfill(xref_promise, stream)?;

        write!(self.backend, "\nstartxref\n{}\n%%EOF", xref_pos).unwrap();

        Ok(&self.backend)
    }
}

#[cfg(feature="cache")]
pub type ObjectCache = Arc<SyncCache<PlainRef, Result<AnySync, Arc<PdfError>>>>;
#[cfg(feature="cache")]
pub type StreamCache = Arc<SyncCache<PlainRef, Result<Arc<[u8]>, Arc<PdfError>>>>;
#[cfg(feature="cache")]
pub type CachedFile<B> = File<B, ObjectCache, StreamCache, NoLog>;

pub struct File<B, OC, SC, L> {
    storage:        Storage<B, OC, SC, L>,
    pub trailer:    Trailer,
}
impl<B, OC, SC, L> Resolve for File<B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    fn resolve_flags(&self, r: PlainRef, flags: ParseFlags, depth: usize) -> Result<Primitive> {
        self.storage.resolve_flags(r, flags, depth)
    }
    fn get<T: Object+DataSize>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        self.storage.get(r)
    }
    fn options(&self) -> &ParseOptions {
        self.storage.options()
    }
    fn get_data_or_decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>> {
        self.storage.get_data_or_decode(id, range, filters)
    }
}
impl<B, OC, SC, L> Updater for File<B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    fn create<T: ObjectWrite>(&mut self, obj: T) -> Result<RcRef<T>> {
        self.storage.create(obj)
    }
    fn update<T: ObjectWrite>(&mut self, old: PlainRef, obj: T) -> Result<RcRef<T>> {
        self.storage.update(old, obj)
    }
    fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        self.storage.promise()
    }
    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>> {
        self.storage.fulfill(promise, obj)
    }
}

impl<OC, SC, L> File<Vec<u8>, OC, SC, L>
where
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    pub fn save_to(&mut self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.storage.save(&mut self.trailer)?)?;
        Ok(())
    }
}


pub struct FileOptions<'a, OC, SC, L> {
    oc: OC,
    sc: SC,
    log: L,
    password: &'a [u8],
    parse_options: ParseOptions,
}
impl FileOptions<'static, NoCache, NoCache, NoLog> {
    pub fn uncached() -> Self {
        FileOptions {
            oc: NoCache,
            sc: NoCache,
            password: b"",
            parse_options: ParseOptions::strict(),
            log: NoLog,
        }
    }
}

#[cfg(feature="cache")]
impl FileOptions<'static, ObjectCache, StreamCache, NoLog> {
    pub fn cached() -> Self {
        FileOptions {
            oc: SyncCache::new(),
            sc: SyncCache::new(),
            password: b"",
            parse_options: ParseOptions::strict(),
            log: NoLog
        }
    }
}
impl<'a, OC, SC, L> FileOptions<'a, OC, SC, L>
where
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    pub fn password(self, password: &'a [u8]) -> FileOptions<'a, OC, SC, L> {
        FileOptions {
            password,
            .. self
        }
    }
    pub fn cache<O, S>(self, oc: O, sc: S) -> FileOptions<'a, O, S, L> {
        let FileOptions { oc: _, sc: _, password, parse_options, log } = self;
        FileOptions {
            oc,
            sc,
            password,
            parse_options,
            log,
        }
    }
    pub fn log<Log>(self, log: Log) -> FileOptions<'a, OC, SC, Log> {
        let FileOptions { oc, sc, password, parse_options, .. } = self;
        FileOptions {
            oc,
            sc,
            password,
            parse_options,
            log,
        }
    }
    pub fn parse_options(self, parse_options: ParseOptions) -> Self {
        FileOptions { parse_options, .. self }
    }

    /// open a file
    pub fn open(self, path: impl AsRef<Path>) -> Result<File<Vec<u8>, OC, SC, L>> {
        let data = std::fs::read(path)?;
        self.load(data)
    }
    pub fn storage(self) -> Storage<Vec<u8>, OC, SC, L> {
        let FileOptions { oc, sc, password, parse_options, log } = self;
        Storage::empty(oc, sc, log)
    }

    /// load data from the given backend
    pub fn load<B: Backend>(self, backend: B) -> Result<File<B, OC, SC, L>> {
        let FileOptions { oc, sc, password, parse_options, log } = self;
        File::load_data(backend, password, parse_options, oc, sc, log)
    }
}


impl<B, OC, SC, L> File<B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    fn load_data(backend: B, password: &[u8], options: ParseOptions, object_cache: OC, stream_cache: SC, log: L) -> Result<Self> {
        let mut storage = Storage::with_cache(backend, options, object_cache, stream_cache, log)?;
        let trailer = storage.load_storage_and_trailer_password(password)?;
        let trailer = t!(Trailer::from_primitive(
            Primitive::Dictionary(trailer),
            &storage,
        ));
        Ok(File { storage, trailer })
    }
    pub fn new(storage: Storage<B, OC, SC, L>, trailer: Trailer) -> Self {
        File { storage, trailer }
    }

    pub fn get_root(&self) -> &Catalog {
        &self.trailer.root
    }

    pub fn pages(&self) -> impl Iterator<Item=Result<PageRc>> + '_ {
        (0 .. self.num_pages()).map(move |n| self.get_page(n))
    }
    pub fn num_pages(&self) -> u32 {
        self.trailer.root.pages.count
    }

    pub fn get_page(&self, n: u32) -> Result<PageRc> {
        self.trailer.root.pages.page(self, n)
    }

    pub fn update_catalog(&mut self, catalog: Catalog) -> Result<()> {
        self.trailer.root = self.create(catalog)?;
        Ok(())
    }

    pub fn set_options(&mut self, options: ParseOptions) {
        self.storage.options = options;
    }

    pub fn scan(&self) -> impl Iterator<Item = Result<ScanItem>> + '_ {
        self.storage.scan()
    }

    pub fn log(&self) -> &L {
        &self.storage.log
    }
}

#[derive(Object, ObjectWrite, DataSize)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub size:               i32,

    #[pdf(key = "Prev")]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               RcRef<Catalog>,

    #[pdf(key = "Encrypt")]
    pub encrypt_dict:       Option<RcRef<CryptDict>>,

    #[pdf(key = "Info", indirect)]
    pub info_dict:          Option<InfoDict>,

    #[pdf(key = "ID")]
    pub id:                 Vec<PdfString>,
}

/*
pub struct XRefStream {
    pub data: Vec<u8>,
    pub info: XRefInfo,
}

impl Object for XRefStream {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let stream = p.to_stream(resolve)?;
        let info = XRefInfo::from_primitive(Primitive::Dictionary (stream.info), resolve)?;
        let data = stream.data.clone();
        Ok(XRefStream {
            data: data,
            info: info,
        })
    }
}
*/
