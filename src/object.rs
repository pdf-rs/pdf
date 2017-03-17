use document::Document;
use file::File;
use primitive::Primitive;
use xref::XRef;
use err::Error;
use std::{io, fmt};
use types::StreamFilter;
use std::marker::PhantomData;
use std::ops::{Deref};

// use std::fmt::{Formatter, Debug};

// Want to wrap file::Primitive together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.

pub trait Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>;
}

pub trait PrimitiveConv: Sized {
    fn from_primitive<B>(p: &Primitive, reader: &File<B>) -> Result<Self, Error>;
}


#[derive(Clone)]
pub struct PlainRef {
    pub id:     u64,
    pub gen:    u32
}
impl Object for PlainRef {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        write!(out, "{} {} R", self.id, self.gen)
    }
}

pub struct PromisedRef<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}

impl<T: Object> Object for PromisedRef<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        self.inner.serialize(out)
    }
}

pub struct Ref<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}
impl<'a, T: Object> From<&'a PromisedRef<T>> for Ref<T> {
    fn from(p: &'a PromisedRef<T>) -> Ref<T> {
        Ref {
            inner:      p.inner,
            _marker:    PhantomData
        }
    }
}
impl<T: Object> Object for Ref<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        self.inner.serialize(out)
    }
}

pub struct RealizedRef<T> {
    inner:      PlainRef,
    obj:        Box<T>
}
impl<T: Object> Deref for RealizedRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.obj
    }
}
impl<T: Object> Object for RealizedRef<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        self.inner.serialize(out)
    }
}


pub struct ObjectStream<'a, W: io::Write + 'a> {
    filters:    Vec<StreamFilter>,
    items:      Vec<usize>,
    data:       Vec<u8>,
    id:         u64,
    file:       &'a mut File<W>
}
impl<'a, W: io::Write + 'a> ObjectStream<'a, W> {
    pub fn new(file: &'a mut File<W>) -> ObjectStream<'a, W> {
        let id = file.promise();
        
        ObjectStream {
            filters:    Vec::new(),
            items:      Vec::new(),
            data:       Vec::new(),
            id:         id,
            file:       file
        }
    }
    pub fn add<T: Object>(&mut self, o: T) -> io::Result<RealizedRef<T>> {
        let start = self.data.len();
        o.serialize(&mut self.data)?;
        let end = self.data.len();
        
        let id = self.file.refs.len() as u64;
        
        self.file.refs.push(XRef::Stream {
            stream_id:  self.id,
            index:      self.items.len() as u32
        });
        
        self.items.push(end - start);
        
        Ok(RealizedRef {
            id:     id,
            obj:    Box::new(o),
        })
    }
    pub fn fulfill<T: Object>(&mut self, promise: PromisedRef<T>, o: T)
     -> io::Result<RealizedRef<T>>
    {
        let start = self.data.len();
        o.serialize(&mut self.data)?;
        let end = self.data.len();
        
        self.file.refs[promise.id as usize] = XRef::Stream {
            stream_id:  self.id,
            index:      self.items.len() as u32
        };
        
        self.items.push(end - start);
        
        Ok(RealizedRef {
            id:     promise.id,
            obj:    Box::new(o),
        })
    }
    pub fn finish(self) -> io::Result<PlainRef> {
        let stream_pos = self.file.cursor.position();
        let ref mut out = self.file.cursor;
        
        write!(out, "{} 0 obj\n", self.id)?;
        let indices = self.items.iter().enumerate().map(|(n, item)| format!("{} {}", n, item)).join(" ");
        
        write_dict!(out,
            "/Type"     << "/ObjStm",
            "/Length"   << self.data.len() + indices.len() + 1,
            "/Filter"   << self.filters,
            "/N"        << self.items.len(),
            "/First"    << indices.len() + 1
        );
        write!(out, "\nstream\n{}\n", indices)?;
        out.write(&self.data)?;
        write!(out, "\nendstream\nendobj\n")?;
        
        
        self.file.refs[self.id as usize] = XRef::Raw {
            offset:  stream_pos
        };
        
        Ok(PlainRef {
            id: self.id
        })
    }
}
