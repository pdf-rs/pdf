//! This is kind of the entry-point of the type-safe PDF functionality.
use std::any::type_name;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::Path;
use std::io::Write;

use crate as pdf;
use crate::error::*;
use crate::object::*;
use crate::primitive::{Dictionary, PdfStream, PdfString, Primitive, StreamInner};
use crate::backend::{Backend, BackendAppend, Data};
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
    fn clear(&self);
    fn remove(&self, key: PlainRef);
}
pub struct NoCache;
impl<T: Clone> Cache<T> for NoCache {
    fn get_or_compute(&self, _key: PlainRef, compute: impl FnOnce() -> T) -> T {
        compute()
    }
    fn clear(&self) {}
    fn remove(&self, _key: PlainRef) {}
}

#[cfg(feature="cache")]
impl<T: Clone + ValueSize + Send + 'static> Cache<T> for Arc<SyncCache<PlainRef, T>> {
    fn get_or_compute(&self, key: PlainRef, compute: impl FnOnce() -> T) -> T {
        self.get(key, |_| compute())
    }
    fn clear(&self) {
        (**self).clear()
    }
    fn remove(&self, key: PlainRef) {
        (**self).remove(&key)
    }
}

pub trait Log {
    fn load_object(&self, _r: PlainRef) {}
    fn log_get(&self, _r: PlainRef) {}
}
#[derive(Clone)]
pub struct NoLog;
impl Log for NoLog {}

#[derive(Clone)]
pub struct Storage<B, OC, SC, L> {
    // objects identical to those in the backend
    cache: OC,
    stream_cache: SC,

    // objects that differ from the backend
    changes:    HashMap<ObjNr, (&'static str, Primitive, GenNr)>,
    promises:   HashMap<ObjNr, &'static str>,

    refs:       XRefTable,

    decoder:    Option<Decoder>,
    options:    ParseOptions,

    backend:    B,

    // Position of the PDF header in the file.
    start_offset: usize,

    log: L
}

const PDF_HEADER: &[u8] = b"%PDF-1.7\n";

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
            promises: HashMap::new(),
            refs: XRefTable::new(0),
            decoder: None,
            options: ParseOptions::strict(),
            backend: Vec::from(PDF_HEADER),
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
    /// The version from the file header ("PDF-1.7" for a file starting with
    /// `%PDF-1.7`). 7.5.1: the header shall be `%PDF-n.m`, so anything else is
    /// an error instead of a slice of whatever bytes follow `%PDF-`.
    pub fn version(&self) -> Result<String> {
        let header = self.backend.read(self.start_offset + 1 .. self.start_offset + 8)?;
        // "PDF-n.m": four literal characters, a digit, '.', a digit
        let well_formed = header.len() == 7
            && &header[..4] == b"PDF-"
            && header[4].is_ascii_digit()
            && header[5] == b'.'
            && header[6].is_ascii_digit();
        if !well_formed {
            bail!(
                "file header is not %PDF-n.m: {:?}",
                String::from_utf8_lossy(header)
            );
        }
        Ok(String::from_utf8(header.to_owned())?)
    }
    pub fn resolver(&self) -> impl Resolve + '_ {
        StorageResolver::new(self)
    }
    pub fn with_cache(backend: B, options: ParseOptions, object_cache: OC, stream_cache: SC, log: L) -> Result<Self> {
        let start_offset = backend.locate_start_offset()?;
        // 7.5.1: a conforming file starts with a %PDF-n.m header. Tolerant
        // parsing still reads files whose version bytes are damaged, but a
        // strict caller gets the error instead of a nonsense version.
        let strict = !options.allow_xref_error;

        let storage = Storage {
            start_offset,
            backend,
            refs: XRefTable::new(0),
            cache: object_cache,
            stream_cache,
            changes: HashMap::new(),
            promises: HashMap::new(),
            decoder: None,
            options,
            log
        };
        if strict {
            t!(storage.version());
        }
        Ok(storage)
    }
    fn decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>> {
        let data = self.backend.read(range)?;

        let mut data = Vec::from(data);
        // 7.6.5: a /Crypt filter in the stream's own filter array replaces the
        // default /StmF for this stream, so the default decryption must not run
        // when one is present — that filter does the work, possibly with
        // /Identity.
        let has_crypt_filter = filters.iter().any(|f| matches!(f, StreamFilter::Crypt(_)));
        if let Some(ref decoder) = self.decoder {
            if !has_crypt_filter {
                data = Vec::from(t!(decoder.decrypt_stream(id, &mut data)));
            }
        }
        for filter in filters {
            match (filter, self.decoder.as_ref()) {
                (StreamFilter::Crypt(params), Some(decoder)) => {
                    data = Vec::from(t!(decoder.decrypt_named(id, &mut data, params.name.as_ref())));
                }
                _ => {
                    data = t!(decode(&data, filter, &self.options), filter);
                }
            }
        }
        Ok(data.into())
    }

    pub fn load_storage_and_trailer(&mut self) -> Result<Dictionary> {
        self.load_storage_and_trailer_password(b"")
    }

    pub fn load_storage_and_trailer_password(&mut self, password: &[u8]) -> Result<Dictionary> {
        // 7.5.5: "The last line of the file shall contain only the
        // end-of-file marker, %%EOF." Tolerant parsing recovers from a
        // truncated or padded tail; a strict caller is told about it.
        if !self.options.allow_xref_error {
            t!(self.check_eof_marker());
        }

        let resolver = StorageResolver::new(self);
        let (refs, trailer) = t!(self.backend.read_xref_table_and_trailer(self.start_offset, &resolver));
        self.refs = refs;

        if let Some(crypt) = trailer.get("Encrypt") {
            let key = trailer
                .get("ID")
                .ok_or(PdfError::MissingEntry {
                    typ: "Trailer",
                    field: "ID".into(),
                })?
                .as_array()?
                .get(0)
                .ok_or(PdfError::MissingEntry {
                    typ: "Trailer",
                    field: "ID[0]".into()
                })?
                .as_string()?
                .as_bytes();

            let resolver = StorageResolver::new(self);
            let dict = CryptDict::from_primitive(crypt.clone(), &resolver)?;

            self.decoder = Some(t!(Decoder::from_password(&dict, key, password)));
            if let Primitive::Reference(reference) = crypt {
                self.decoder.as_mut().unwrap().encrypt_indirect_object = Some(*reference);
            }
            if let Some(Primitive::Reference(catalog_ref)) = trailer.get("Root") {
                let resolver = StorageResolver::new(self);
                let catalog = t!(t!(resolver.resolve(*catalog_ref)).resolve(&resolver)?.into_dictionary());
                if let Some(Primitive::Reference(metadata_ref)) = catalog.get("Metadata") {
                    self.decoder.as_mut().unwrap().metadata_indirect_object = Some(*metadata_ref);
                }
            }
        }
        Ok(trailer)
    }
    /// 7.5.5: the file shall end with the `%%EOF` marker, and nothing but
    /// whitespace may follow it. Only the last kilobyte is inspected, which is
    /// where `startxref` and the marker are expected to live.
    fn check_eof_marker(&self) -> Result<()> {
        let len = self.backend.len();
        let from = std::cmp::max(self.start_offset, len.saturating_sub(1024));
        let tail = self.backend.read(from .. len)?;
        match tail.windows(5).rposition(|w| w == b"%%EOF") {
            Some(pos) if tail[pos + 5 ..].iter().all(|b| b.is_ascii_whitespace()) => Ok(()),
            Some(pos) => bail!(
                "data follows the %%EOF marker: {:?}",
                String::from_utf8_lossy(&tail[pos + 5 ..])
            ),
            None => bail!("the file does not end with a %%EOF marker"),
        }
    }

    pub fn scan(&self) -> impl Iterator<Item = Result<ScanItem>> + '_ {
        // `scan` walks the file body between the header and the cross-reference
        // section. A caller may do this on a damaged file, so a missing
        // startxref must be an error in the iterator, not a panic.
        let mut error = None;
        let slice = match self.backend.locate_xref_offset()
            .and_then(|xref_offset| self.backend.read(self.start_offset .. xref_offset))
        {
            Ok(slice) => slice,
            Err(e) => {
                error = Some(e);
                &[][..]
            }
        };
        let mut lexer = Lexer::with_offset(slice, 0);

        fn skip_xref(lexer: &mut Lexer) -> Result<()> {
            while lexer.next()? != "trailer" {

            }
            Ok(())
        }

        let resolver = StorageResolver::new(self);
        std::iter::from_fn(move || {
            if let Some(e) = error.take() {
                return Some(Err(e));
            }
            loop {
                let pos = lexer.get_pos();
                match parse_indirect_object(&mut lexer, &resolver, self.decoder.as_ref(), ParseFlags::all()) {
                    Ok((r, p)) => return Some(Ok(ScanItem::Object(r, p))),
                    Err(e) if e.is_eof() => return None,
                    Err(e) => {
                        lexer.set_pos(pos);
                        if let Ok(s) = lexer.next() {
                            debug!("next: {:?}", String::from_utf8_lossy(s.as_slice()));
                            match &*s {
                                b"xref" => {
                                    if let Err(e) = skip_xref(&mut lexer) {
                                        return Some(Err(e));
                                    }
                                    if let Ok(trailer) = parse_with_lexer(&mut lexer, &NoResolve, ParseFlags::DICT).and_then(|p| p.into_dictionary()) {
                                        return Some(Ok(ScanItem::Trailer(trailer)));
                                    }
                                }
                                b"startxref" if lexer.next().is_ok() => {
                                    continue;
                                }
                                _ => {}
                            }
                        }
                        return Some(Err(e));
                    }
                }
            }
        })
    }
    fn resolve_ref(&self, r: PlainRef, flags: ParseFlags, resolve: &impl Resolve) -> Result<Primitive> {
        match self.changes.get(&r.id) {
            Some((_, p, _)) => Ok((*p).clone()),
            None => match t!(self.refs.get(r.id)) {
                XRef::Raw {pos, ..} => {
                    let mut lexer = Lexer::with_offset(t!(self.backend.read(self.start_offset + pos ..), r), self.start_offset + pos);
                    let p = t!(parse_indirect_object(&mut lexer, resolve, self.decoder.as_ref(), flags), r).1;
                    Ok(p)
                }
                XRef::Stream {stream_id, index} => {
                    if !flags.contains(ParseFlags::STREAM) {
                        return Err(PdfError::PrimitiveNotAllowed { found: ParseFlags::STREAM, allowed: flags });
                    }
                    // use get to cache the object stream
                    let obj_stream = resolve.get::<ObjectStream>(Ref::from_id(stream_id))?;

                    // 7.5.7: the type 2 entry gives a position in the stream, and
                    // the stream's header repeats the object number at that
                    // position. When the two disagree one of them is wrong; the
                    // header is the more specific statement, so trust it before
                    // giving up.
                    let index = match obj_stream.object_nr(index) {
                        Some(nr) if nr == r.id => index,
                        Some(nr) => match obj_stream.index_of(r.id) {
                            Some(found) => found,
                            None => {
                                return Err(PdfError::Other { msg: format!(
                                    "object {} is not in object stream {} (position {} holds object {})",
                                    r.id, stream_id, index, nr
                                )})
                            }
                        },
                        None => {
                            return Err(PdfError::ObjStmOutOfBounds {
                                index,
                                max: obj_stream.n_objects(),
                            })
                        }
                    };

                    let (data, range) = t!(obj_stream.get_object_slice(index, resolve));
                    let slice = data.get(range.clone()).ok_or_else(|| other!("invalid range {:?}, but only have {} bytes", range, data.len()))?;
                    // 7.5.7: "an object stream shall not contain stream
                    // objects"; such an object is to be treated as null.
                    match parse(slice, resolve, flags) {
                        Err(PdfError::PrimitiveNotAllowed { found: ParseFlags::STREAM, .. }) => {
                            Ok(Primitive::Null)
                        }
                        other => other,
                    }
                }
                XRef::Free {..} => err!(PdfError::FreeObject {obj_nr: r.id}),
                XRef::Promised => err!(PdfError::Other { msg: "object is only promised, never written".into() }),
                XRef::Invalid => err!(PdfError::NullRef {obj_nr: r.id}),
            }
        }
    }
}

pub enum ScanItem {
    Object(PlainRef, Primitive),
    Trailer(Dictionary)
}

struct StorageResolver<'a, B, OC, SC, L> {
    storage: &'a Storage<B, OC, SC, L>,
    chain: Mutex<Vec<PlainRef>>,
}
impl<'a, B, OC, SC, L> StorageResolver<'a, B, OC, SC, L> {
    pub fn new(storage: &'a Storage<B, OC, SC, L>) -> Self {
        StorageResolver {
            storage,
            chain: Mutex::new(vec![])
        }
    }
}

struct Defer<F: FnMut()>(F);
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

fn resolve_get<'a, B, OC, SC, L, T>(resolver: &impl Resolve, storage: &'a Storage<B, OC, SC, L>, chain: &Mutex<Vec<PlainRef>>, r: Ref<T>) -> Result<RcRef<T>>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
    T: Object+DataSize
{
    let key = r.get_inner();
    storage.log.log_get(key);

    {
        debug!("get {key:?} as {}", std::any::type_name::<T>());
        let mut chain = chain.lock().unwrap();
        if chain.contains(&key) {
            bail!("Recursive reference");
        }
        chain.push(key);
    }
    let _defer = Defer(|| {
        let mut chain = chain.lock().unwrap();
        assert_eq!(chain.pop(), Some(key));
    });

    let res = storage.cache.get_or_compute(key, || {
        match resolver.resolve(key).and_then(|p| T::from_primitive(p, resolver)) {
            Ok(obj) => Ok(AnySync::new(Shared::new(obj))),
            Err(e) => {
                let p = resolver.resolve(key);
                warn!("failed to decode {p:?} as {}", std::any::type_name::<T>());
                Err(Arc::new(e))
            }
        }
    });
    match res {
        Ok(any) => {
            match any.downcast() {
                Ok(val) => Ok(RcRef::new(key, val)),
                Err(_) => {
                    let p = resolver.resolve(key)?;
                    Ok(RcRef::new(key, T::from_primitive(p, resolver)?.into()))
                }
            }
        }
        Err(e) => Err(PdfError::Shared { source: e.clone()}),
    }
}

impl<'a, B, OC, SC, L> Resolve for StorageResolver<'a, B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log
{
    fn resolve_flags(&self, r: PlainRef, flags: ParseFlags, _depth: usize) -> Result<Primitive> {
        let storage = self.storage;
        storage.log.load_object(r);

        storage.resolve_ref(r, flags, self)
    }

    fn get<T: Object+DataSize>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        resolve_get(self, &self.storage, &self.chain, r)
    }
    fn options(&self) -> &ParseOptions {
        &self.storage.options
    }
    fn stream_data(&self, id: PlainRef, range: Range<usize>) -> Result<Arc<[u8]>> {
        self.storage.decode(id, range, &[])
    }
    fn get_data_or_decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>> {
        self.storage.stream_cache.get_or_compute(id, || self.storage.decode(id, range, filters).map_err(Arc::new))
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
        self.changes.insert(id, (std::any::type_name::<T>(), primitive, 0));
        let rc = Shared::new(obj);
        let r = PlainRef { id, gen: 0 };

        Ok(RcRef::new(r, rc))
    }
    fn update_with<T: ObjectWrite>(&mut self, old: PlainRef, obj: T, options: UpdateOptions) -> Result<RcRef<T>> {
        use std::collections::hash_map::Entry;

        let r = match self.refs.get(old.id)? {
            // A free or null object has no generation to keep and no slot to
            // write into (7.5.4); that is a caller error, not a reason to abort
            // the process.
            XRef::Free { .. } => err!(other!("object {} is free and can not be updated", old.id)),
            XRef::Raw { gen_nr, .. } => PlainRef { id: old.id, gen: gen_nr },
            XRef::Stream { .. } => return self.create(obj),
            XRef::Promised => PlainRef { id: old.id, gen: 0 },
            XRef::Invalid => err!(PdfError::NullRef { obj_nr: old.id }),
        };
        let primitive = obj.to_primitive(self)?;
        match self.changes.entry(old.id) {
            Entry::Vacant(e) => {
                e.insert((std::any::type_name::<T>(), primitive, r.gen));
            }
            Entry::Occupied(mut e) => {
                if options.merge {
                    match (e.get_mut(), primitive) {
                        ((_, Primitive::Dictionary(ref mut dict), _), Primitive::Dictionary(new)) => {
                            dict.append(new);
                        }
                        ((old_name, old_prim, _), new_prim) => {
                            bail!(
                                "can't update previous type {} which is {} with data from new type {} which is {}",
                                old_name, old_prim.get_debug_name(), std::any::type_name::<T>(), new_prim.get_debug_name()
                            );
                        }
                    }
                } else {
                    e.insert((std::any::type_name::<T>(), primitive, r.gen));
                }
            }
        }
        let rc = Shared::new(obj);
        self.cache.remove(old);

        Ok(RcRef::new(r, rc))
    }

    fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        let id = self.refs.len() as u64;

        self.refs.push(XRef::Promised);
        self.promises.insert(id, type_name::<T>());

        PromisedRef {
            inner: PlainRef {
                id,
                gen: 0
            },
            _marker:    PhantomData
        }
    }

    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>> {
        let r = self.update(promise.inner, obj)?;
        self.promises.remove(&promise.inner.id);
        Ok(r)
    }
}

#[derive(Default)]
pub struct UpdateOptions {
    pub merge: bool
}

/// Options for [`Storage::rebuild`].
#[derive(Clone, Debug)]
pub struct RebuildOptions {
    /// Pack the objects that are not streams into object streams (7.5.7)
    /// instead of writing each of them at the top level. Object numbers do not
    /// change, only where the object is stored.
    pub object_streams: bool,

    /// Maximum number of objects in one object stream.
    pub object_stream_max_objects: usize,

    /// Maximum number of bytes (before compression) in one object stream.
    pub object_stream_max_bytes: usize,

    /// Objects to leave out of the output. They are still used while writing
    /// (an object stream that is excluded still yields its members), and they
    /// end up as free entries in the new cross-reference stream (7.5.4).
    pub exclude: Vec<ObjNr>,
}

impl Default for RebuildOptions {
    fn default() -> Self {
        RebuildOptions {
            object_streams: true,
            object_stream_max_objects: 1000,
            object_stream_max_bytes: 64 * 1024,
            exclude: Vec::new(),
        }
    }
}

/// The dictionary of an object stream (7.5.7, Table 16).
#[derive(Object, ObjectWrite, Debug, DataSize)]
#[pdf(Type = "ObjStm")]
struct ObjStmInfo {
    /// The number of objects in the stream.
    #[pdf(key = "N")]
    n: u32,

    /// The byte offset of the first object, relative to the start of the
    /// decoded stream data.
    #[pdf(key = "First")]
    first: u32,
}

impl<OC, SC, L, B> Storage<B, OC, SC, L>
where
    B: BackendAppend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log
{
    /// Replace object `id` with a stream holding `data`, keeping its object
    /// number. The data is stored unfiltered, so drop any existing `/Filter`
    /// from `info` first.
    pub fn replace_with_stream(&mut self, id: ObjNr, info: Dictionary, data: impl Into<Arc<[u8]>>) -> Result<()> {
        let stream = PdfStream::new_memory(info, data);
        self.replace_with_primitive(id, Primitive::Stream(stream));
        Ok(())
    }

    /// Replace the object `id` resolves to. The object number is kept, so
    /// references to it stay valid.
    pub fn replace_with_primitive(&mut self, id: ObjNr, prim: Primitive) {
        let r = PlainRef { id, gen: 0 };
        self.cache.remove(r);
        self.stream_cache.remove(r);
        self.changes.insert(id, ("replacement", prim, 0));
    }

    /// Write the whole document to a new byte buffer.
    ///
    /// Objects that are not streams are packed into object streams (7.5.7), so
    /// the result is roughly the size of the input rather than the size of its
    /// uncompressed object dictionaries. Like the cross-reference stream that is
    /// written here, that needs PDF 1.5.
    pub fn rebuild(&mut self, trailer: &mut Trailer) -> Result<Vec<u8>> {
        self.rebuild_with(trailer, &RebuildOptions::default())
    }

    /// `rebuild`, with control over the object streams.
    ///
    /// The trailer that gets written is the one that is passed in, so the caller
    /// decides about /ID and /Info. /Size is set here, /Prev is dropped: it
    /// points into the file that is being rewritten. [`Trailer::normalise`]
    /// clears the parts that make two copies of a document differ.
    pub fn rebuild_with(&mut self, trailer: &mut Trailer, options: &RebuildOptions) -> Result<Vec<u8>> {
        if self.promises.len() > 0 {
            for (id, name) in self.promises.iter() {
                eprintln!("Promise unfulfilled: id={id} type={name}");
                return Err(PdfError::Invalid);
            }
        }

        // 7.5.5: /Prev is the offset of the previous cross-reference section in
        // *this* file. A full rewrite has none.
        trailer.prev_trailer_pos = None;

        let mut out = Vec::new();
        out.extend_from_slice(PDF_HEADER);

        // Resolve every object once, in ascending object number. Streams have to
        // stay top level (7.5.7), everything else goes into an object stream.
        let mut objstms: Vec<(Vec<(ObjNr, usize)>, Vec<u8>)> = Vec::new();
        let mut members: Vec<(ObjNr, usize)> = Vec::new();
        let mut data: Vec<u8> = Vec::new();
        let mut top: Vec<(ObjNr, Primitive)> = Vec::new();
        let mut excluded: Vec<ObjNr> = Vec::new();
        {
            let max_objects = options.object_stream_max_objects.max(1);
            let max_bytes = options.object_stream_max_bytes.max(1);
            let resolver = StorageResolver::new(self);
            for (obj_nr, r) in self.refs.entries() {
                let id = obj_nr as ObjNr;
                let prim = t!(self.resolve_ref(PlainRef { id, gen: 0 }, ParseFlags::ANY, &resolver), id, r);

                // resolved anyway: an object stream that is dropped has to stay
                // readable for the objects it contains
                if options.exclude.contains(&id) {
                    excluded.push(id);
                    continue;
                }

                if options.object_streams && !matches!(prim, Primitive::Stream(_)) {
                    members.push((id, data.len()));
                    t!(prim.serialize(&mut data));
                    data.push(b'\n');
                    if members.len() >= max_objects || data.len() >= max_bytes {
                        objstms.push((
                            std::mem::replace(&mut members, Vec::new()),
                            std::mem::replace(&mut data, Vec::new())
                        ));
                    }
                } else {
                    top.push((id, prim));
                }
            }
        }
        if !members.is_empty() {
            objstms.push((members, data));
        }

        // Each object stream is an indirect object of its own. Its number has to
        // be above every number in use, so nothing the trailer adds can collide
        // with it.
        let mut objstm_ids = Vec::with_capacity(objstms.len());
        for _ in 0..objstms.len() {
            objstm_ids.push(self.refs.len() as ObjNr);
            self.refs.push(XRef::Invalid);
        }

        // writing the trailer generates another id for the info dictionary
        trailer.size = (self.refs.len() + 1) as _;
        let mut trailer_dict = trailer.to_dict(self)?;
        let xref_promise = self.promise::<Stream<XRefInfo>>();
        trailer.size = self.refs.len() as i32;
        trailer_dict.insert("Size", Primitive::Integer(trailer.size as _));
        if trailer.id.is_empty() {
            // 7.5.5: /ID is optional, but if it is there it is an array of two
            // byte strings. An empty one is not.
            trailer_dict.remove("ID");
        }

        let mut xrefs: Vec<(ObjNr, XRef)> = Vec::new();
        {
            let resolver = StorageResolver::new(self);
            for &(id, ref prim) in top.iter() {
                let byte_pos = out.len();
                writeln!(out, "{id} 0 obj")?;
                match prim {
                    Primitive::Stream(PdfStream { inner: StreamInner::InFile { file_range, .. }, info }) => {
                        let data = self.backend.read(file_range.clone())?;

                        let len_val = try_opt!(info.get("Length")).clone();
                        let info_len = t!(len_val.resolve(&resolver)).as_usize()?;
                        assert_eq!(info_len, data.len());
                        t!(info.serialize(&mut out));
                        writeln!(out, "stream")?;
                        out.write_all(&data)?;
                        writeln!(out, "\nendstream")?;
                    }
                    prim => {
                        t!(prim.serialize(&mut out));
                    }
                }

                writeln!(out, "endobj")?;
                xrefs.push((id, XRef::Raw { pos: byte_pos, gen_nr: 0 }));
            }
            for (&id, &(_, ref prim, _)) in self.changes.iter() {
                if options.exclude.contains(&id) {
                    continue;
                }
                match self.refs.get(id) {
                    Ok(XRef::Raw { .. }) | Ok(XRef::Stream { .. }) => continue,
                    _ => {}
                }
                let byte_pos = out.len();
                writeln!(out, "{id} 0 obj")?;
                prim.serialize(&mut out)?;
                if !out.last_byte().map(|b| b.is_ascii_whitespace()).unwrap_or(false) {
                    out.write_all(b"\n")?;
                }
                writeln!(out, "endobj")?;
                xrefs.push((id, XRef::Raw { pos: byte_pos, gen_nr: 0 }));
            }

            // 7.5.7: the object stream holds a header of `obj_nr offset` pairs
            // (offsets relative to /First) followed by the objects themselves.
            for ((members, data), &id) in objstms.iter().zip(objstm_ids.iter()) {
                let mut header = String::new();
                for (obj_nr, offset) in members.iter() {
                    header.push_str(&format!("{} {} ", obj_nr, offset));
                }
                let first = header.len();
                let mut payload = Vec::with_capacity(first + data.len());
                payload.extend_from_slice(header.as_bytes());
                payload.extend_from_slice(data);

                let info = ObjStmInfo { n: members.len() as u32, first: first as u32 };
                let stream = t!(Stream::new(info, &payload));

                let byte_pos = out.len();
                writeln!(out, "{id} 0 obj")?;
                let pdf_stream = t!(stream.to_pdf_stream(&mut NoUpdate));
                t!(pdf_stream.serialize(&mut out));
                writeln!(out, "endobj")?;

                xrefs.push((id, XRef::Raw { pos: byte_pos, gen_nr: 0 }));
                for (index, &(obj_nr, _)) in members.iter().enumerate() {
                    xrefs.push((obj_nr, XRef::Stream { stream_id: id, index }));
                }
            }
        }
        for id in excluded {
            self.refs.set(id, XRef::Free { next_obj_nr: 0, gen_nr: 0 });
        }
        for (id, xref) in xrefs {
            self.refs.set(id, xref);
        }

        let xref_pos = out.len();
        self.refs.set(xref_promise.get_inner().id, XRef::Raw { pos: xref_pos, gen_nr: 0 });
        // only write up to the xref stream obj id
        let size = self.refs.len();
        let xref_stream = t!(self.refs.write_stream(size));

        writeln!(out, "{} {} obj", xref_promise.get_inner().id, 0)?;
        let mut xref_and_trailer = xref_stream.to_pdf_stream(&mut NoUpdate)?;
        for (k, v) in trailer_dict.iter() {
            xref_and_trailer.info.insert(k.clone(), v.clone());
        }

        xref_and_trailer.serialize(&mut out)?;
        writeln!(out, "endobj")?;

        let _ = self.fulfill(xref_promise, xref_stream)?;

        write!(out, "\nstartxref\n{}\n%%EOF", xref_pos).unwrap();

        // update trailer which may have change now.
        *trailer = t!(Trailer::from_dict(trailer_dict, &self.resolver()));
        self.cache.clear();

        Ok(out)
    }
    pub fn save(&mut self, trailer: &mut Trailer) -> Result<()> {
        if self.promises.len() > 0 {
            for (id, name) in self.promises.iter() {
                eprintln!("Promise unfulfilled: id={id} type={name}");
                return Err(PdfError::Invalid);
            }
        }

        // writing the trailer generates another id for the info dictionary
        trailer.size = (self.refs.len() + 2) as _;
        let trailer_dict = trailer.to_dict(self)?;

        let xref_promise = self.promise::<Stream<XRefInfo>>();

        let mut changes: Vec<_> = self.changes.iter().collect();
        changes.sort_unstable_by_key(|&(id, _)| id);

        for &(&id, &(_name, ref primitive, gen)) in changes.iter() {
            let pos = self.backend.len();
            self.refs.set(id, XRef::Raw { pos: pos as _, gen_nr: gen });
            writeln!(self.backend, "{id} {gen} obj")?;
            primitive.serialize(&mut self.backend)?;
            if !self.backend.last_byte().map(|b| b.is_ascii_whitespace()).unwrap_or(false) {
                self.backend.write_all(b"\n")?;
            }
            writeln!(self.backend, "endobj")?;
        }

        let xref_pos = self.backend.len();
        self.refs.set(xref_promise.get_inner().id, XRef::Raw { pos: xref_pos, gen_nr: 0 });
        // only write up to the xref stream obj id
        let stream = self.refs.write_stream(xref_promise.get_inner().id as usize + 1)?;

        writeln!(self.backend, "{} {} obj", xref_promise.get_inner().id, 0)?;
        let mut xref_and_trailer = stream.to_pdf_stream(&mut NoUpdate)?;
        for (k, v) in trailer_dict.iter() {
            xref_and_trailer.info.insert(k.clone(), v.clone());
        }

        xref_and_trailer.serialize(&mut self.backend)?;
        writeln!(self.backend, "endobj")?;

        let _ = self.fulfill(xref_promise, stream)?;

        write!(self.backend, "\nstartxref\n{}\n%%EOF", xref_pos).unwrap();

        // update trailer which may have change now.
        self.cache.clear();
        *trailer = Trailer::from_dict(trailer_dict, &self.resolver())?;

        Ok(())
    }
}

#[cfg(feature="cache")]
pub type ObjectCache = Arc<SyncCache<PlainRef, Result<AnySync, Arc<PdfError>>>>;
#[cfg(feature="cache")]
pub type StreamCache = Arc<SyncCache<PlainRef, Result<Arc<[u8]>, Arc<PdfError>>>>;
#[cfg(feature="cache")]
pub type CachedFile<B> = File<B, ObjectCache, StreamCache, NoLog>;

pub struct File<B, OC, SC, L> {
    pub storage:    Storage<B, OC, SC, L>,
    pub trailer:    Trailer,
    resolve_chain:  Mutex<Vec<PlainRef>>,
}
impl<B: Clone, OC: Clone, SC: Clone, L: Clone> Clone for File<B, OC, SC, L> {
    fn clone(&self) -> Self {
        File {
            storage: self.storage.clone(),
            trailer: self.trailer.clone(),
            resolve_chain: Default::default()
        }
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
    fn update_with<T: ObjectWrite>(&mut self, old: PlainRef, obj: T, options: UpdateOptions) -> Result<RcRef<T>> {
        self.storage.update_with(old, obj, options)
    }
    fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        self.storage.promise()
    }
    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>> {
        self.storage.fulfill(promise, obj)
    }
}
impl<B, OC, SC, L> Resolve for File<B, OC, SC, L>
where
    B: Backend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    fn resolve_flags(&self, r: PlainRef, flags: ParseFlags, _depth: usize) -> Result<Primitive> {
        let ref storage = self.storage;
        storage.log.load_object(r);

        storage.resolve_ref(r, flags, self)
    }

    fn get<T: Object+DataSize>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        resolve_get(self, &self.storage, &self.resolve_chain, r)
    }
    fn options(&self) -> &ParseOptions {
        &self.storage.options
    }
    fn stream_data(&self, id: PlainRef, range: Range<usize>) -> Result<Arc<[u8]>> {
        self.storage.decode(id, range, &[])
    }
    fn get_data_or_decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>> {
        self.storage.stream_cache.get_or_compute(id, || self.storage.decode(id, range, filters).map_err(Arc::new))
        .map_err(|e| e.into())
    }
}
impl<B, OC, SC, L> File<B, OC, SC, L>
where
    B: BackendAppend,
    OC: Cache<Result<AnySync, Arc<PdfError>>>,
    SC: Cache<Result<Arc<[u8]>, Arc<PdfError>>>,
    L: Log,
{
    pub fn save_to(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.storage.save(&mut self.trailer)?;
        let bytes = self.storage.backend.read(..)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
    pub fn rebuild(&mut self) -> Result<Vec<u8>> {
        let data = self.storage.rebuild(&mut self.trailer)?;
        Ok(data)
    }
}


/// Upper bound for `FileOptions::open` when the caller does not choose one.
/// A document that is opened is either read into memory or mapped, so the
/// default is a limit on what one document may cost, not a statement about
/// valid PDFs.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024;

/// How [`FileOptions::open`] gets the bytes of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// Map the file when the `mmap` feature is enabled, read it otherwise.
    /// Empty files are always read (there is nothing to map). A file system
    /// that refuses the mapping falls back to reading as well.
    Auto,
    /// Read the whole file into memory. Never fails for a file that exists.
    Read,
    /// Map the file. Errors if the build has no `mmap` feature, and for a
    /// zero length file.
    Mapped,
}
impl Default for OpenMode {
    fn default() -> Self {
        OpenMode::Auto
    }
}

pub struct FileOptions<'a, OC, SC, L> {
    oc: OC,
    sc: SC,
    log: L,
    password: &'a [u8],
    parse_options: ParseOptions,
    max_file_size: Option<u64>,
    open_mode: OpenMode,
}
impl FileOptions<'static, NoCache, NoCache, NoLog> {
    pub fn uncached() -> Self {
        FileOptions {
            oc: NoCache,
            sc: NoCache,
            password: b"",
            parse_options: ParseOptions::strict(),
            log: NoLog,
            max_file_size: Some(DEFAULT_MAX_FILE_SIZE),
            open_mode: OpenMode::default(),
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
            log: NoLog,
            max_file_size: Some(DEFAULT_MAX_FILE_SIZE),
            open_mode: OpenMode::default(),
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
        let FileOptions { oc: _, sc: _, password, parse_options, log, max_file_size, open_mode } = self;
        FileOptions {
            oc,
            sc,
            password,
            parse_options,
            log,
            max_file_size,
            open_mode,
        }
    }
    pub fn log<Log>(self, log: Log) -> FileOptions<'a, OC, SC, Log> {
        let FileOptions { oc, sc, password, parse_options, max_file_size, open_mode, .. } = self;
        FileOptions {
            oc,
            sc,
            password,
            parse_options,
            log,
            max_file_size,
            open_mode,
        }
    }
    pub fn parse_options(self, parse_options: ParseOptions) -> Self {
        FileOptions { parse_options, .. self }
    }

    /// Refuse to `open` a file larger than `limit` bytes (default:
    /// `DEFAULT_MAX_FILE_SIZE`). `None` removes the limit; `load` with a
    /// caller-provided backend is never limited.
    pub fn max_file_size(self, limit: Option<u64>) -> Self {
        FileOptions { max_file_size: limit, .. self }
    }

    /// Choose how `open` gets the bytes (default: `OpenMode::Auto`, i.e. map
    /// the file when the `mmap` feature is enabled).
    pub fn open_mode(self, mode: OpenMode) -> Self {
        FileOptions { open_mode: mode, .. self }
    }

    /// Open a file. With the `mmap` feature the bytes are mapped rather than
    /// read, so a large document does not have to fit in memory; the returned
    /// [`Data`] says which happened (`is_mapped`). Reading and editing work
    /// either way: the first write copies the bytes into memory.
    pub fn open(self, path: impl AsRef<Path>) -> Result<File<Data, OC, SC, L>> {
        let path = path.as_ref();
        // an absurd input is refused before it can cost anything: a mapping
        // reserves address space, a read allocates memory
        let size = std::fs::metadata(path)?.len();
        if let Some(limit) = self.max_file_size {
            if size > limit {
                bail!("{} is {} bytes, over the {} byte limit", path.display(), size, limit);
            }
        }
        let data = match self.open_mode {
            OpenMode::Read => Data::Owned(std::fs::read(path)?),
            OpenMode::Mapped => t!(Data::map_file(path)),
            OpenMode::Auto => if size == 0 {
                Data::Owned(Vec::new())
            } else {
                match Data::map_file(path) {
                    Ok(data) => data,
                    Err(e) => {
                        // no `mmap` feature, or a file system that can not map:
                        // reading is always possible
                        debug!("{}: {}; reading it into memory", path.display(), e);
                        Data::Owned(std::fs::read(path)?)
                    }
                }
            },
        };
        self.load(data)
    }

    /// `open`, but the file is always read into memory.
    pub fn open_owned(self, path: impl AsRef<Path>) -> Result<File<Vec<u8>, OC, SC, L>> {
        let path = path.as_ref();
        if let Some(limit) = self.max_file_size {
            let size = std::fs::metadata(path)?.len();
            if size > limit {
                bail!("{} is {} bytes, over the {} byte limit", path.display(), size, limit);
            }
        }
        let data = std::fs::read(path)?;
        self.load(data)
    }

    pub fn storage(self) -> Storage<Vec<u8>, OC, SC, L> {
        let FileOptions { oc, sc, log, .. } = self;
        Storage::empty(oc, sc, log)
    }

    /// load data from the given backend
    pub fn load<B: Backend>(self, backend: B) -> Result<File<B, OC, SC, L>> {
        let FileOptions { oc, sc, password, parse_options, log, max_file_size: _, open_mode: _ } = self;
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
    pub fn load_data(backend: B, password: &[u8], options: ParseOptions, object_cache: OC, stream_cache: SC, log: L) -> Result<Self> {
        let mut storage = Storage::with_cache(backend, options, object_cache, stream_cache, log)?;
        let trailer = storage.load_storage_and_trailer_password(password)?;

        let resolver = StorageResolver::new(&storage);
        let trailer = t!(Trailer::from_primitive(
            Primitive::Dictionary(trailer),
            &resolver,
        ));
        Ok(File { storage, trailer, resolve_chain: Default::default() })
    }
    pub fn new(storage: Storage<B, OC, SC, L>, trailer: Trailer) -> Self {
        File { storage, trailer, resolve_chain: Default::default() }
    }
    pub fn resolver(&self) -> impl Resolve + '_ {
        StorageResolver::new(&self.storage)
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
        let resolver = StorageResolver::new(&self.storage);
        self.trailer.root.pages.page(&resolver, n)
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

    /// the version string in the file header.
    /// if the version field in the Catalog is set, this should be used instead.
    pub fn version(&self) -> Result<String> {
        self.storage.version()
    }
}

#[derive(Clone, Object, ObjectWrite, DataSize)]
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

impl Trailer {
    /// Drop the entries that only describe where this copy of the document came
    /// from, so that rewriting the same document twice gives the same bytes: the
    /// previous cross-reference section (7.5.5) and the file identifier (14.4).
    /// Set a new /ID afterwards if the document needs one.
    pub fn normalise(&mut self) {
        self.prev_trailer_pos = None;
        self.id.clear();
    }
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
