//! This is kind of the entry-point of the type-safe PDF functionality.
use std::{str};
use std::marker::PhantomData;
use std::collections::HashMap;
use err::*;
use object::*;
use xref::{XRef, XRefTable};
use primitive::{Primitive, Dictionary, PdfString};
use backend::Backend;

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

// tail call
fn find_page<'a>(pages: &'a PageTree, mut offset: i32, page_nr: i32) -> Result<&'a Page> {
    for kid in &pages.kids {
        // println!("{}/{} {:?}", offset, page_nr, kid);
        match *kid {
            PagesNode::Tree(ref t) => {
                if offset + t.count < page_nr {
                    offset += t.count;
                } else {
                    return find_page(t, offset, page_nr);
                }
            },
            PagesNode::Leaf(ref p) => {
                if offset < page_nr {
                    offset += 1;
                } else {
                    assert_eq!(offset, page_nr);
                    return Ok(p);
                }
            }
        }
    }
    Err(ErrorKind::PageNotFound {page_nr: page_nr}.into())
}
    
// tail call to trick borrowck
fn update_pages(pages: &mut PageTree, mut offset: i32, page_nr: i32, page: Page) -> Result<()>  {
    for kid in &mut pages.kids.iter_mut() {
        // println!("{}/{} {:?}", offset, page_nr, kid);
        match *kid {
            PagesNode::Tree(ref mut t) => {
                if offset + t.count < page_nr {
                    offset += t.count;
                } else {
                    return update_pages(t, offset, page_nr, page);
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
    Err(ErrorKind::PageNotFound {page_nr: page_nr}.into())
}


pub struct File<B: Backend> {
    backend:    B,
    trailer:    Trailer,
    refs:       XRefTable,
    changes:    HashMap<ObjNr, Primitive>
}

impl<B: Backend> File<B> {
    pub fn new(b: B) -> File<B> {
        File {
            backend:    b,
            trailer:    Trailer::default(),
            refs:       XRefTable::new(1), // the root object,
            changes:    HashMap::new()
        }
    }
    pub fn open(path: &str) -> Result<File<B>> {
        let backend = B::open(path)?;
        let (refs, trailer) = backend.read_xref_table_and_trailer()?;
        let trailer = Trailer::from_primitive(Primitive::Dictionary(trailer), &|r| backend.resolve(&refs, r))?;
        //
        
        Ok(File {
            backend:    backend,
            trailer:    trailer,
            refs:       refs,
            changes:    HashMap::new()
        })
    }


    pub fn get_root(&self) -> &Catalog {
        &self.trailer.root
    }

    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        match self.changes.get(&r.id) {
            Some(ref p) => Ok((*p).clone()),
            None => self.backend.resolve(&self.refs, r)
        }
    }

    pub fn deref<T: Object>(&self, r: Ref<T>) -> Result<T> {
        let primitive = self.resolve(r.get_inner())?;
        T::from_primitive(primitive, &|id| self.resolve(id))
    }
    pub fn get_num_pages(&self) -> Result<i32> {
        Ok(self.trailer.root.pages.count)
    }
    pub fn get_page(&self, n: i32) -> Result<&Page> {
        if n >= self.get_num_pages()? {
            return Err(ErrorKind::PageOutOfBounds {page_nr: n, max: self.get_num_pages()?}.into());
        }
        find_page(&self.trailer.root.pages, 0, n)
    }
    
    pub fn update_page(&mut self, page_nr: i32, page: Page) -> Result<()> {
        update_pages(&mut self.trailer.root.pages, 0, page_nr, page)
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

}


#[derive(Object, Default)]
#[pdf(Type=false)]
pub struct Trailer {
    #[pdf(key = "Size")]
    pub highest_id:         i32,

    #[pdf(key = "Prev")]
    pub prev_trailer_pos:   Option<i32>,

    #[pdf(key = "Root")]
    pub root:               Catalog,

    #[pdf(key = "Encrypt")]
    pub encrypt_dict:       Option<Dictionary>,

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
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
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
