use document::Document;
use file::File;
use primitive::{Primitive, Dictionary, Stream};
use xref::XRef;
use err::{Result, ErrorKind};
use std::{io, fmt};
use types::StreamFilter;
use std::marker::PhantomData;
use std::ops::{Deref};

// use std::fmt::{Formatter, Debug};

// Want to wrap file::Primitive together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.

pub type ObjNr = u64;
pub type GenNr = u16;
pub type Resolve = Fn(PlainRef) -> Result<Primitive>;

/// Resolve function that just throws an error
pub const no_resolve: &'static Resolve =  &|plain_ref| {
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

impl<'a, T> Object for &'a T where T: Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        self.serialize(out)
    }
}

#[derive(Clone, Debug)]
pub struct PlainRef {
    pub id:     ObjNr,
    pub gen:    GenNr,
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


