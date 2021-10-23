//! This is kind of the entry-point of the type-safe PDF functionality.
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::marker::PhantomData;
use std::path::Path;
use std::rc::Rc;

use crate as pdf;
use crate::any::Any;
use crate::backend::Backend;
use crate::crypt::CryptDict;
use crate::crypt::Decoder;
use crate::error::*;
use crate::object::*;
use crate::parser::Lexer;
use crate::parser::{parse, parse_indirect_object};
use crate::primitive::{Dictionary, PdfString, Primitive};
use crate::xref::{XRef, XRefInfo, XRefTable};

#[must_use]
pub struct PromisedRef<T> {
    inner:   PlainRef,
    _marker: PhantomData<T>,
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
    cache: RefCell<HashMap<PlainRef, Any>>,

    // objects that differ from the backend
    changes: HashMap<ObjNr, Primitive>,

    refs: XRefTable,

    decoder: Option<Decoder>,

    backend: B,

    // Position of the PDF header in the file.
    start_offset: usize,
}
impl<B: Backend> Storage<B> {
    pub fn new(backend: B, refs: XRefTable, start_offset: usize) -> Storage<B> {
        Storage {
            backend,
            refs,
            start_offset,
            cache: RefCell::new(HashMap::new()),
            changes: HashMap::new(),
            decoder: None,
        }
    }
}
impl<B: Backend> Resolve for Storage<B> {
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        match self.changes.get(&r.id) {
            Some(p) => Ok(p.clone()),
            None => match t!(self.refs.get(r.id)) {
                XRef::Raw { pos, .. } => {
                    let mut lexer = Lexer::new(t!(self.backend.read(self.start_offset + pos..)));
                    let p = t!(parse_indirect_object(
                        &mut lexer,
                        self,
                        self.decoder.as_ref()
                    ))
                    .1;
                    Ok(p)
                }
                XRef::Stream { stream_id, index } => {
                    let obj_stream = t!(self.resolve(PlainRef {
                        id:  stream_id,
                        gen: 0, /* TODO what gen nr? */
                    }));
                    let obj_stream = t!(ObjectStream::from_primitive(obj_stream, self));
                    let slice = t!(obj_stream.get_object_slice(index));
                    parse(slice, self)
                }
                XRef::Free { .. } => err!(PdfError::FreeObject { obj_nr: r.id }),
                XRef::Promised => unimplemented!(),
                XRef::Invalid => err!(PdfError::NullRef { obj_nr: r.id }),
            },
        }
    }
    fn get<T: Object>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        let key = r.get_inner();

        if let Some(any) = self.cache.borrow().get(&key) {
            return Ok(RcRef::new(key, any.clone().downcast()?));
        }

        let primitive = t!(self.resolve(key));
        let obj = t!(T::from_primitive(primitive, self));
        let rc = Rc::new(obj);
        self.cache.borrow_mut().insert(key, Any::new(rc.clone()));

        Ok(RcRef::new(key, rc))
    }
}
impl<B: Backend> Updater for Storage<B> {
    fn create<T: ObjectWrite>(&mut self, obj: T) -> Result<RcRef<T>> {
        let id = self.refs.len() as u64;
        self.refs.push(XRef::Promised);
        let primitive = obj.to_primitive(self)?;
        self.changes.insert(id, primitive);
        let rc = Rc::new(obj);
        let r = PlainRef { id, gen: 0 };

        Ok(RcRef::new(r, rc))
    }
    fn update<T: ObjectWrite>(&mut self, old: PlainRef, obj: T) -> Result<RcRef<T>> {
        let r = match self.refs.get(old.id)? {
            XRef::Free { .. } => panic!(),
            XRef::Raw { gen_nr, .. } => PlainRef {
                id:  old.id,
                gen: gen_nr + 1,
            },
            XRef::Stream { .. } => return self.create(obj),
            XRef::Promised => PlainRef {
                id:  old.id,
                gen: 0,
            },
            XRef::Invalid => panic!(),
        };
        let primitive = obj.to_primitive(self)?;
        self.changes.insert(old.id, primitive);
        let rc = Rc::new(obj);

        Ok(RcRef::new(r, rc))
    }

    fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        let id = self.refs.len() as u64;

        self.refs.push(XRef::Promised);

        PromisedRef {
            inner:   PlainRef { id, gen: 0 },
            _marker: PhantomData,
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
            self.refs.set(
                id,
                XRef::Raw {
                    pos:    pos as _,
                    gen_nr: 0,
                },
            );
            writeln!(&mut self.backend, "{} {} obj", id, 0)?;
            primitive.serialize(&mut self.backend, 0)?;
            writeln!(self.backend, "endobj")?;
        }

        let xref_pos = self.backend.len();

        // only write up to the xref stream obj id
        let stream = self.refs.write_stream(xref_promise.get_inner().id as _)?;

        writeln!(
            &mut self.backend,
            "{} {} obj",
            xref_promise.get_inner().id,
            0
        )?;
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

pub fn load_storage_and_trailer<B: Backend>(backend: B) -> Result<(Storage<B>, Dictionary)> {
    load_storage_and_trailer_password(backend, b"")
}

pub fn load_storage_and_trailer_password<B: Backend>(
    backend: B,
    password: &[u8],
) -> Result<(Storage<B>, Dictionary)> {
    let start_offset = t!(backend.locate_start_offset());
    let (refs, trailer) = t!(backend.read_xref_table_and_trailer(start_offset));
    let mut storage = Storage::new(backend, refs, start_offset);

    if let Some(crypt) = trailer.get("Encrypt") {
        let key = trailer
            .get("ID")
            .ok_or(PdfError::MissingEntry {
                typ:   "Trailer",
                field: "ID".into(),
            })?
            .as_array()?[0]
            .as_string()?
            .as_bytes();
        let dict = CryptDict::from_primitive(crypt.clone(), &storage)?;
        storage.decoder = Some(t!(Decoder::from_password(&dict, key, password)));
        if let Primitive::Reference(reference) = crypt {
            storage.decoder.as_mut().unwrap().encrypt_indirect_object = Some(*reference);
        }
        if let Some(Primitive::Reference(catalog_ref)) = trailer.get("Root") {
            let catalog = t!(t!(storage.resolve(*catalog_ref)).into_dictionary(&storage));
            if let Some(Primitive::Reference(metadata_ref)) = catalog.get("Metadata") {
                storage.decoder.as_mut().unwrap().metadata_indirect_object = Some(*metadata_ref);
            }
        }
    }
    Ok((storage, trailer))
}

pub struct File<B: Backend> {
    storage:     Storage<B>,
    pub trailer: Trailer,
}
impl<B: Backend> Resolve for File<B> {
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        self.storage.resolve(r)
    }
    fn get<T: Object>(&self, r: Ref<T>) -> Result<RcRef<T>> {
        self.storage.get(r)
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
        Self::load_data(backend, password)
    }

    pub fn from_data(backend: B) -> Result<Self> {
        Self::from_data_password(backend, b"")
    }

    fn load_data(backend: B, password: &[u8]) -> Result<Self> {
        let (storage, trailer) = load_storage_and_trailer_password(backend, password)?;
        let trailer = t!(Trailer::from_primitive(
            Primitive::Dictionary(trailer),
            &storage,
        ));
        Ok(File { storage, trailer })
    }

    pub fn get_root(&self) -> &Catalog {
        &self.trailer.root
    }

    pub fn pages(&'_ self) -> impl Iterator<Item = Result<PageRc>> + '_ {
        (0..self.num_pages()).map(move |n| self.get_page(n))
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
}

#[derive(Object, ObjectWrite)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id: i32,

    #[pdf(key = "Prev")]
    pub prev_trailer_pos: Option<i32>,

    #[pdf(key = "Root")]
    pub root: RcRef<Catalog>,

    #[pdf(key = "Encrypt")]
    pub encrypt_dict: Option<RcRef<CryptDict>>,

    #[pdf(key = "Info")]
    pub info_dict: Option<Dictionary>,

    #[pdf(key = "ID")]
    pub id: Vec<PdfString>,
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
