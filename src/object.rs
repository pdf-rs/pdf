use primitive::{Primitive, Dictionary, Stream};
use err::{Result, ErrorKind};
use std::io;
use std::marker::PhantomData;
use std::ops::{Deref};

// use std::fmt::{Formatter, Debug};

// Want to wrap file::Primitive together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.

pub type ObjNr = u64;
pub type GenNr = u16;
pub type Resolve<'a> = Fn(PlainRef) -> Result<&'a Primitive>;

/// Resolve function that just throws an error
pub const NO_RESOLVE: &'static Resolve =  &|_| {
    Err(ErrorKind::FollowReference.into())
};

pub trait Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>;
}


pub trait PrimitiveConv: Sized {
    fn from_primitive(p: &Primitive, resolve: &Resolve) -> Result<Self>;
}
pub trait FromDict: Sized {
    fn from_dict(dict: &Dictionary, resolve: &Resolve) -> Result<Self>;
}
pub trait FromStream: Sized {
    fn from_stream(dict: &Stream, resolve: &Resolve) -> Result<Self>;
}

#[derive(Copy, Clone, Debug)]
pub struct PlainRef {
    pub id:     ObjNr,
    pub gen:    GenNr,
}
pub struct Ref<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}
impl<T> Ref<T> {
    pub fn new(inner: PlainRef) -> Ref<T> {
        Ref {
            inner:      inner,
            _marker:    PhantomData::default(),
        }
    }
}


/// Either a reference or the object itself.
pub enum MaybeRef<T> {
    Owned (T),
    Reference (Ref<T>),
}
pub struct PromisedRef<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}
pub struct RealizedRef<T> {
    inner:      PlainRef,
    obj:        Box<T>
}


impl<'a, T> Object for &'a T where T: Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        unimplemented!();
    }
}

impl Object for PlainRef {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        write!(out, "{} {} R", self.id, self.gen)
    }
}


impl<T: Object> Object for PromisedRef<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        self.inner.serialize(out)
    }
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
impl<T: Object> Object for MaybeRef<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        match *self {
            MaybeRef::Owned (ref obj) => obj.serialize(out),
            MaybeRef::Reference (ref r) => r.serialize(out),
        }
    }
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

impl Object for Dictionary {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "<<")?;
        for (key, val) in self.iter() {
            write!(out, "/{} ", key);
            val.serialize(out)?;
        }
        write!(out, ">>")
    }
}
impl Object for Primitive {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        match *self {
            Primitive::Null => write!(out, "null"),
            Primitive::Integer (ref x) => x.serialize(out),
            Primitive::Number (ref x) => x.serialize(out),
            Primitive::Boolean (ref x) => x.serialize(out),
            Primitive::String (_) => unimplemented!(),
            Primitive::Stream (_) => unimplemented!(),
            Primitive::Dictionary (ref x) => x.serialize(out),
            Primitive::Array (ref x) => x.serialize(out),
            Primitive::Reference (ref x) => x.serialize(out),
            Primitive::Name (ref x) => x.serialize(out),
        }
    }
}



