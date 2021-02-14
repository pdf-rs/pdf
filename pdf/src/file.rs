//! This is kind of the entry-point of the type-safe PDF functionality.
use std::fs;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::path::Path;
use std::io::Write;

use crate as pdf;
use crate::error::*;
use crate::object::*;
use crate::primitive::{Primitive, Dictionary, PdfString};
use crate::backend::Backend;
use crate::any::{Any};
use crate::parser::Lexer;
use crate::parser::{parse_indirect_object, parse};
use crate::xref::{XRef, XRefTable};
use crate::crypt::Decoder;
use crate::crypt::CryptDict;

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
    cache: RefCell<HashMap<PlainRef, Any>>,

    // objects that differ from the backend
    changes:    HashMap<ObjNr, Primitive>,

    refs:       XRefTable,

    decoder:    Option<Decoder>,

    backend:    B,

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
            Some(ref p) => Ok((*p).clone()),
            None => match t!(self.refs.get(r.id)) {
                XRef::Raw {pos, ..} => {
                    let mut lexer = Lexer::new(t!(self.backend.read(self.start_offset + pos ..)));
                    let p = t!(parse_indirect_object(&mut lexer, self, self.decoder.as_ref())).1;
                    Ok(p)
                }
                XRef::Stream {stream_id, index} => {
                    let obj_stream = t!(self.resolve(PlainRef {id: stream_id, gen: 0 /* TODO what gen nr? */}));
                    let obj_stream = t!(ObjectStream::from_primitive(obj_stream, self));
                    let slice = t!(obj_stream.get_object_slice(index));
                    parse(slice, self)
                }
                XRef::Free {..} => err!(PdfError::FreeObject {obj_nr: r.id}),
                XRef::Promised => unimplemented!(),
                XRef::Invalid => err!(PdfError::NullRef {obj_nr: r.id}),
            }
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
            XRef::Raw { gen_nr, .. } => PlainRef { id: old.id, gen: gen_nr + 1 },
            XRef::Stream { .. } => return self.create(obj),
            XRef::Promised => PlainRef { id: old.id, gen: 0 },
            XRef::Invalid => panic!()
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
            inner: PlainRef {
                id:     id,
                gen:    0
            },
            _marker:    PhantomData
        }
    }
    
    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>> {
        self.update(promise.inner, obj)
    }
}

impl Storage<Vec<u8>> {
    pub fn save(&mut self, trailer: &Trailer) -> Result<&[u8]> {
        let trailer = trailer.to_primitive(self)?;

        let mut changes: Vec<_> = self.changes.iter().collect();
        changes.sort_unstable_by_key(|&(id, _)| id);

        for (&id, primitive) in changes.iter() {
            let pos = self.backend.len();
            self.refs.set(id, XRef::Raw { pos: pos as _, gen_nr: 0 });
            primitive.serialize(&mut self.backend)?;
        }

        let xref_pos = self.backend.len();
        write!(self.backend, "xref\n{} {}\n", 0, self.refs.len()).unwrap();
        self.refs.write_old_format(&mut self.backend);

        write!(self.backend, "trailer\n").unwrap();
        trailer.serialize(&mut self.backend).unwrap();

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
                typ: "Trailer",
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
    }
    Ok((storage, trailer))
}

pub struct File<B: Backend> {
    storage:    Storage<B>,
    pub trailer:    Trailer,
    pub version: u8,
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
        std::fs::write(path, self.storage.save(&self.trailer)?)?;
        Ok(())
    }
}
impl<B: Backend> File<B> {
    pub fn from_data_password(backend: B, password: &[u8]) -> Result<Self> {
        Self::load_data(backend, password).map_err(|e| dbg!(e))
    }

    pub fn from_data(backend: B) -> Result<Self> {
        Self::from_data_password(backend, b"")
    }

    fn load_data(backend: B, password: &[u8]) -> Result<Self> {
        let header: &[u8] = backend.read(0 .. 8)?;
        if !(header[0 .. 7] == *b"%PDF-1." && (b'0' ..= b'7').contains(&header[7])) {
            bail!("not a PDF");
        }
        let version = header[7] - b'0';
        let (storage, trailer) = load_storage_and_trailer_password(backend, password)?;
        let trailer = t!(Trailer::from_primitive(
            Primitive::Dictionary(trailer),
            &storage,
        ));
        Ok(File { storage, trailer, version })
    }

    pub fn get_root(&self) -> &Catalog {
        &self.trailer.root
    }

    pub fn pages<'a>(&'a self) -> impl Iterator<Item=Result<PageRc>> + 'a {
        (0 .. self.num_pages()).map(move |n| self.get_page(n))
    }
    pub fn num_pages(&self) -> u32 {
        self.trailer.root.pages.count
    }

    pub fn get_page(&self, n: u32) -> Result<PageRc> {
        self.trailer.root.pages.page(self, n)
    }

    pub fn update_catalog(&mut self, catalog: Catalog) {
        self.trailer.highest_id = self.storage.refs.len() as _;
        self.trailer.root = catalog;
    }
}

    
#[derive(Object, ObjectWrite)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id:         i32,

    #[pdf(key = "Prev")]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               Catalog,

    #[pdf(key = "Encrypt")]
    pub encrypt_dict:       Option<RcRef<CryptDict>>,

    #[pdf(key = "Info")]
    pub info_dict:          Option<Dictionary>,

    #[pdf(key = "ID")]
    pub id:                 Vec<PdfString>,
}

#[derive(Object, Debug)]
#[pdf(Type = "XRef")]
pub struct XRefInfo {
    // XRefStream fields
    #[pdf(key = "Size")]
    pub size: i32,

    //
    #[pdf(key = "Index", default = "vec![0, size]")]
    /// Array of pairs of integers for each subsection, (first object number, number of entries).
    /// Default value (assumed when None): `(0, self.size)`.
    pub index: Vec<i32>,

    #[pdf(key = "Prev")]
    prev: Option<i32>,

    #[pdf(key = "W")]
    pub w: Vec<i32>
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
