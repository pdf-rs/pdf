use primitive::{Primitive, Dictionary, Stream};
use err::{Result, ErrorKind};
use std::io;
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref};
use types::write_list;

pub type ObjNr = u64;
pub type GenNr = u16;
pub trait Resolve: {
    fn resolve(&self, r: PlainRef) -> Result<Primitive>;
}
impl<F> Resolve for F where F: Fn(PlainRef) -> Result<Primitive> {
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        self(r)
    }
}


/// Resolve function that just throws an error
pub struct NoResolve {}
impl Resolve for NoResolve {
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        Err(ErrorKind::FollowReference.into())
    }
}
pub const NO_RESOLVE: &'static Resolve = &NoResolve {} as &Resolve;

pub trait Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>;
}


pub trait FromPrimitive: Sized {
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self>;
}
pub trait FromDict: Sized {
    fn from_dict(dict: Dictionary, resolve: &Resolve) -> Result<Self>;
}
pub trait FromStream: Sized {
    fn from_stream(dict: Stream, resolve: &Resolve) -> Result<Self>;
}




/* PlainRef */
#[derive(Copy, Clone, Debug)]
pub struct PlainRef {
    pub id:     ObjNr,
    pub gen:    GenNr,
}
impl Object for PlainRef {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        write!(out, "{} {} R", self.id, self.gen)
    }
}


/* Ref<T> */
// NOTE: Copy & Clone implemented manually ( https://github.com/rust-lang/rust/issues/26925 )
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
    pub fn from_id(id: ObjNr) -> Ref<T> {
        Ref {
            inner:      PlainRef {id: id, gen: 0},
            _marker:    PhantomData::default(),
        }
    }
    pub fn get_inner(&self) -> PlainRef {
        self.inner
    }
}
impl<T: Object> Object for Ref<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        self.inner.serialize(out)
    }
}
impl<T> FromPrimitive for Ref<T> {
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        Ok(Ref::new(p.as_reference()?))
    }
}

impl<T> Copy for Ref<T> { }

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Ref<T> {
        *self
    }
}
impl<T> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ref({})", self.inner.id)
    }
}




////////////////////////
// Other Object impls //
////////////////////////

impl Object for i32 {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }
}
impl Object for f32 {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }
}
impl Object for bool {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
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
impl Object for str {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        for b in self.chars() {
            match b {
                '\\' | '(' | ')' => write!(out, r"\")?,
                c if c > '~' => panic!("only ASCII"),
                _ => ()
            }
            write!(out, "{}", b)?;
        }
        Ok(())
    }
}
impl Object for String {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        (self as &str).serialize(out)
    }
}

impl<T: Object> Object for Vec<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write_list(out, self.iter())
    }
}
impl<T: Object> Object for [T] {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write_list(out, self.iter())
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

impl<'a, T> Object for &'a T where T: Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        unimplemented!();
    }
}
