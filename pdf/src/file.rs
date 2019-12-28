//! This is kind of the entry-point of the type-safe PDF functionality.
use std::fs;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::path::Path;

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

pub struct PromisedRef<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}
impl<'a, T> Into<PlainRef> for &'a PromisedRef<T> {
    fn into(self) -> PlainRef {
        self.inner
    }
}
impl<'a, T> Into<Ref<T>> for &'a PromisedRef<T> {
    fn into(self) -> Ref<T> {
        Ref::new(self.into())
    }
}

pub struct Storage<B: Backend> {
    // objects identical to those in the backend
    cache: RefCell<HashMap<PlainRef, Any>>,
    
    // objects that differ from the backend
    changes:    HashMap<ObjNr, Primitive>,
    
    refs:       XRefTable,
    
    decoder:    Option<Decoder>,
    
    backend: B
}
impl<B: Backend> Storage<B> {
    pub fn new(backend: B, refs: XRefTable) -> Storage<B> {
        Storage {
            backend,
            refs,
            cache: RefCell::new(HashMap::new()),
            changes: HashMap::new(),
            decoder: None
        }
    }
}
impl<B: Backend> Resolve for Storage<B> {
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        match self.changes.get(&r.id) {
            Some(ref p) => Ok((*p).clone()),
            None => match self.refs.get(r.id)? {
                XRef::Raw {pos, ..} => {
                    let mut lexer = Lexer::new(self.backend.read(pos..)?);
                    let p = parse_indirect_object(&mut lexer, self, self.decoder.as_ref())?.1;
                    Ok(p)
                }
                XRef::Stream {stream_id, index} => {
                    let obj_stream = self.resolve(PlainRef {id: stream_id, gen: 0 /* TODO what gen nr? */})?;
                    let obj_stream = ObjectStream::from_primitive(obj_stream, self)?;
                    let slice = obj_stream.get_object_slice(index)?;
                    parse(slice, self)
                }
                XRef::Free {..} => err!(PdfError::FreeObject {obj_nr: r.id}),
                XRef::Promised => unimplemented!(),
                XRef::Invalid => err!(PdfError::NullRef {obj_nr: r.id}),
            }
        }
    }
    fn get<T: Object>(&self, r: Ref<T>) -> Result<Rc<T>> {
        let key = r.get_inner();
        
        if let Some(any) = self.cache.borrow().get(&key) {
            return any.clone().downcast();
        }
        
        let primitive = self.resolve(r.get_inner())?;
        let obj = T::from_primitive(primitive, self)?;
        let rc = Rc::new(obj);
        self.cache.borrow_mut().insert(key, Any::new(rc.clone()));
        
        Ok(rc)
    }
}

pub struct File<B: Backend> {
    storage:    Storage<B>,
    pub trailer:    Trailer,
}
impl<B: Backend> Resolve for File<B> {
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        self.storage.resolve(r)
    }
    fn get<T: Object>(&self, r: Ref<T>) -> Result<Rc<T>> {
        self.storage.get(r)
    }
}
impl File<Vec<u8>> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_data(fs::read(path)?)
    }
}
impl<B: Backend> File<B> {
    /// Opens the file at `path` and uses Vec<u8> as backend.
    pub fn from_data(backend: B) -> Result<Self> {
        let (refs, trailer) = backend.read_xref_table_and_trailer()?;
        let mut storage = Storage::new(backend, refs);

        let trailer = Trailer::from_primitive(Primitive::Dictionary(trailer), &storage)?;
        if let Some(ref dict) = trailer.encrypt_dict {
            storage.decoder = Some(Decoder::default(&dict, trailer.id[0].as_bytes())?);
        }
        
        Ok(File {
            storage,
            trailer,
        })
    }
    

    pub fn get_root(&self) -> &Catalog {
        &self.trailer.root
    }
    
    pub fn pages<'a>(&'a self) -> impl Iterator<Item=Result<PageRc>> + 'a {
        (0 .. self.num_pages()).map(move |n| self.get_page(n))
    }
    pub fn num_pages(&self) -> u32 {
        match self.trailer.root.pages {
            PagesNode::Tree(ref tree) => tree.count,
            PagesNode::Leaf(_) => 1
        }
    }
    
    pub fn get_page(&self, n: u32) -> Result<PageRc> {
        match self.trailer.root.pages {
            PagesNode::Tree(ref tree) => {
                tree.page(self, n)
            }
            PagesNode::Leaf(ref page) if n == 0 => Ok(page.clone()),
            _ => Err(PdfError::PageOutOfBounds {page_nr: n, max: 1})
        }
    }

    /*
    pub fn get_images(&self) -> Vec<ImageXObject> {
        let mut images = Vec::<ImageXObject>::new();
        scan_pages(&self.trailer.root.pages, 0, &mut |page| {
            println!("Found page!");
            match page.resources {
                Some(ref res) => {
                    match res.xobject {
                        Some(ref xobjects) => {
                            for (name, xobject) in xobjects {
                                match *xobject {
                                    XObject::Image (ref img_xobject) => {
                                        images.push(img_xobject.clone())
                                    }
                                    _ => {},
                                }
                            }
                        },
                        None => {},
                    }
                },
                None => {},
            }
        });
        images
    }
    
    // tail call to trick borrowck
    fn update_pages(&self, pages: &mut PageTree, mut offset: i32, page_nr: i32, page: Page) -> Result<()>  {
        for kid in &mut pages.kids.iter_mut() {
            // println!("{}/{} {:?}", offset, page_nr, kid);
            match *(self.get(kid)?) {
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
                        *p = page;
                        return Ok(());
                    }
                }
            }
            
        }
        Err(PdfError::PageNotFound {page_nr: page_nr})
    }
    
    pub fn update_page(&mut self, page_nr: i32, page: Page) -> Result<()> {
        self.update_pages(&mut self.trailer.root.pages, 0, page_nr, page)
    }
    
    pub fn update(&mut self, id: ObjNr, primitive: Primitive) {
        self.changes.insert(id, primitive);
    }
    
    pub fn promise<T: Object>(&mut self) -> PromisedRef<T> {
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
    
    pub fn fulfill<T>(&mut self, promise: PromisedRef<T>, obj: T) -> Ref<T>
    where T: Into<Primitive>
    {
        self.update(promise.inner.id, obj.into());
        
        Ref::new(promise.inner)
    }
    
    pub fn add<T>(&mut self, obj: T) -> Ref<T> where T: Into<Primitive> {
        let id = self.refs.len() as u64;
        self.refs.push(XRef::Promised);
        self.update(id, obj.into());
        
        Ref::from_id(id)
    }
    */
}

    
#[derive(Object)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id:         i32,

    #[pdf(key = "Prev")]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               Catalog,

    #[pdf(key = "Encrypt")]
    pub encrypt_dict:       Option<CryptDict>,

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
