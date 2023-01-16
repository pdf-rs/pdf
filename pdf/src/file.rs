//! This is kind of the entry-point of the type-safe PDF functionality.
use std::fs;
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
use crate::parser::Lexer;
use crate::parser::{parse_indirect_object, parse, ParseFlags};
use crate::xref::{XRef, XRefTable, XRefInfo};
use crate::crypt::Decoder;
use crate::crypt::CryptDict;
use crate::enc::{StreamFilter, decode};
use std::ops::Range;
use globalcache::sync::SyncCache;
use datasize::DataSize;

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

pub struct Storage<B: Backend> {
    // objects identical to those in the backend
    cache: Arc<SyncCache<PlainRef, Result<AnySync, Arc<PdfError>>>>,
    stream_cache: Arc<SyncCache<PlainRef, Result<Arc<[u8]>, Arc<PdfError>>>>,

    // objects that differ from the backend
    changes:    HashMap<ObjNr, Primitive>,

    refs:       XRefTable,

    decoder:    Option<Decoder>,
    options:    ParseOptions,

    backend:    B,

    // Position of the PDF header in the file.
    start_offset: usize,
}
impl<B: Backend> Storage<B> {
    fn new(backend: B, options: ParseOptions) -> Result<Storage<B>> {
        Ok(Storage {
            start_offset: backend.locate_start_offset()?,
            backend,
            refs: XRefTable::new(0),
            cache: SyncCache::new(),
            stream_cache: SyncCache::new(),
            changes: HashMap::new(),
            decoder: None,
            options,
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
}
impl<B: Backend> Resolve for Storage<B> {
    fn resolve_flags(&self, r: PlainRef, flags: ParseFlags, depth: usize) -> Result<Primitive> {
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
                    let obj_stream = t!(self.resolve_flags(PlainRef {id: stream_id, gen: 0 /* TODO what gen nr? */}, flags, depth-1));
                    let obj_stream = t!(ObjectStream::from_primitive(obj_stream, self));
                    let (data, range) = t!(obj_stream.get_object_slice(index, self));
                    parse(&data[range], self, flags)
                }
                XRef::Free {..} => err!(PdfError::FreeObject {obj_nr: r.id}),
                XRef::Promised => unimplemented!(),
                XRef::Invalid => err!(PdfError::NullRef {obj_nr: r.id}),
            }
        }
    }

    fn get<T: Object+DataSize>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        let key = r.get_inner();
        
        let res = self.cache.get(key, || {
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
        self.stream_cache.get(id, || self.decode(id, range, filters).map_err(Arc::new))
        .map_err(|e| e.into())
    }
}
impl<B: Backend> Updater for Storage<B> {
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

impl Storage<Vec<u8>> {
    pub fn save(&mut self, trailer: &mut Trailer) -> Result<&[u8]> {
        let xref_promise = self.promise::<Stream<XRefInfo>>();

        trailer.highest_id = self.refs.len() as _;
        let trailer = trailer.to_dict(self)?;

        let mut changes: Vec<_> = self.changes.iter().collect();
        changes.sort_unstable_by_key(|&(id, _)| id);

        for (&id, primitive) in changes.iter() {
            let pos = self.backend.len();
            self.refs.set(id, XRef::Raw { pos: pos as _, gen_nr: 0 });
            writeln!(self.backend, "{} {} obj", id, 0)?;
            primitive.serialize(&mut self.backend, 0)?;
            writeln!(self.backend, "endobj")?;
        }

        let xref_pos = self.backend.len();

        // only write up to the xref stream obj id
        let stream = self.refs.write_stream(xref_promise.get_inner().id as _)?;

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

pub fn load_storage_and_trailer<B: Backend>(storage: &mut Storage<B>) -> Result<Dictionary>

{
    load_storage_and_trailer_password(storage, b"")
}

pub fn load_storage_and_trailer_password<B: Backend>(
    storage: &mut Storage<B>,
    password: &[u8],
) -> Result<Dictionary> {
    let (refs, trailer) = t!(storage.backend.read_xref_table_and_trailer(storage.start_offset, storage));
    storage.refs = refs;

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
        let dict = CryptDict::from_primitive(crypt.clone(), storage)?;
        storage.decoder = Some(t!(Decoder::from_password(&dict, key, password)));
        if let Primitive::Reference(reference) = crypt {
            storage.decoder.as_mut().unwrap().encrypt_indirect_object = Some(*reference);
        }
        if let Some(Primitive::Reference(catalog_ref)) = trailer.get("Root") {
            let catalog = t!(t!(storage.resolve(*catalog_ref)).resolve(storage)?.into_dictionary());
            if let Some(Primitive::Reference(metadata_ref)) = catalog.get("Metadata") {
                storage.decoder.as_mut().unwrap().metadata_indirect_object = Some(*metadata_ref);
            }
        }
    }
    Ok(trailer)
}

pub struct File<B: Backend> {
    storage:    Storage<B>,
    pub trailer:    Trailer,
}
impl<B: Backend> Resolve for File<B> {
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
impl<B: Backend> Updater for File<B> {
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

impl File<Vec<u8>> {
    /// Opens the file at `path` and uses Vec<u8> as backend.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_data(fs::read(path)?)
    }

    /// Opens the file at `path`, with a password, and uses Vec<u8> as backend.
    pub fn open_password(path: impl AsRef<Path>, password: &[u8]) -> Result<Self> {
        Self::from_data_password(fs::read(path)?, password)
    }

    pub fn save_to(&mut self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.storage.save(&mut self.trailer)?)?;
        Ok(())
    }
}
impl<B: Backend> File<B> {
    pub fn from_data_password(backend: B, password: &[u8]) -> Result<Self> {
        Self::load_data(backend, password, ParseOptions::strict())
    }
    pub fn from_data_password_with_options(backend: B, password: &[u8], options: ParseOptions) -> Result<Self> {
        Self::load_data(backend, password, options)
    }

    pub fn from_data(backend: B) -> Result<Self> {
        Self::load_data(backend, b"", ParseOptions::strict())
    }
    pub fn from_data_with_options(backend: B, options: ParseOptions) -> Result<Self> {
        Self::load_data(backend, b"", options)
    }

    fn load_data(backend: B, password: &[u8], options: ParseOptions) -> Result<Self> {
        let mut storage = Storage::new(backend, options)?;
        let trailer = load_storage_and_trailer_password(&mut storage, password)?;
        let trailer = t!(Trailer::from_primitive(
            Primitive::Dictionary(trailer),
            &storage,
        ));
        Ok(File { storage, trailer })
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
}

#[derive(Object, ObjectWrite, DataSize)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id:         i32,

    #[pdf(key = "Prev")]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               RcRef<Catalog>,

    #[pdf(key = "Encrypt")]
    pub encrypt_dict:       Option<RcRef<CryptDict>>,

    #[pdf(key = "Info")]
    pub info_dict:          Option<Dictionary>,

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
