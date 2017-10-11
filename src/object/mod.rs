//! `Object` trait, along with some implementations. References.
//!
//! Some of the structs are incomplete (missing fields that are in the PDF references).

mod types;
mod stream;

pub use self::types::*;
pub use self::stream::*;

use primitive::*;
use err::*;
use enc::*;

use std::io;
use std::fmt;
use std::marker::PhantomData;
use std::collections::BTreeMap;

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


pub struct NoResolve {}
impl Resolve for NoResolve {
    fn resolve(&self, _: PlainRef) -> Result<Primitive> {
        Err(ErrorKind::FollowReference.into())
    }
}
/// Resolve function that just throws an error
pub const NO_RESOLVE: &'static Resolve = &NoResolve {} as &Resolve;

/// A PDF Object
pub trait Object: Sized {
    /// Write object as a byte stream
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>;
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self>;
}

///////
// Refs
///////

// TODO move to primitive.rs
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PlainRef {
    pub id:     ObjNr,
    pub gen:    GenNr,
}
impl Object for PlainRef {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()>  {
        write!(out, "{} {} R", self.id, self.gen)
    }
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        p.to_reference()
    }
}


// NOTE: Copy & Clone implemented manually ( https://github.com/rust-lang/rust/issues/26925 )
#[derive(Copy,Clone)]
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
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        Ok(Ref::new(p.to_reference()?))
    }
}

impl<T> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ref({})", self.inner.id)
    }
}

//////////////////////////////////////
// Object for Primitives & other types
//////////////////////////////////////

impl Object for i32 {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        Ok(
        match p {
            Primitive::Integer (n) => n,
            Primitive::Reference (r) => i32::from_primitive(resolve.resolve(r)?, resolve)?,
            p => bail!(Error::from(ErrorKind::UnexpectedPrimitive {expected: "Integer", found: p.get_debug_name()}))
        }
        )
    }
}
impl Object for usize {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        Ok(i32::from_primitive(p, r)? as usize)
    }
}
impl Object for f32 {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        p.as_number()
    }
}
impl Object for bool {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{}", self)
    }
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        p.as_bool()
    }
}
impl Object for Dictionary {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "<<")?;
        for (key, val) in self.iter() {
            write!(out, "/{} ", key)?;
            val.serialize(out)?;
        }
        write!(out, ">>")
    }
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        match p {
            Primitive::Dictionary(dict) => Ok(dict),
            Primitive::Reference(id) => Dictionary::from_primitive(r.resolve(id)?, r),
            _ => bail!(ErrorKind::UnexpectedPrimitive {expected: "Dictionary", found: p.get_debug_name()}),
        }
    }
}

impl Object for String {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        for b in self.as_str().chars() {
            match b {
                '\\' | '(' | ')' => write!(out, r"\")?,
                c if c > '~' => panic!("only ASCII"),
                _ => ()
            }
            write!(out, "{}", b)?;
        }
        Ok(())
    }
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        Ok(p.to_name()?)
    }
}

impl<T: Object> Object for Vec<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write_list(out, self.iter())
    }
    /// Will try to convert `p` to `T` first, then try to convert `p` to Vec<T>
    fn from_primitive(p: Primitive, r: &Resolve) -> Result<Self> {
        Ok(
        match p {
            Primitive::Array(_) => {
                p.to_array(r)?
                    .into_iter()
                    .map(|p| T::from_primitive(p, r))
                    .collect::<Result<Vec<T>>>()?
            },
            Primitive::Null => {
                Vec::new()
            }
            _ => vec![T::from_primitive(p, r)?]
        }
        )
    }
}

impl Object for Primitive {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        match *self {
            Primitive::Null => write!(out, "null"),
            Primitive::Integer (ref x) => x.serialize(out),
            Primitive::Number (ref x) => x.serialize(out),
            Primitive::Boolean (ref x) => x.serialize(out),
            Primitive::String (ref x) => x.serialize(out),
            Primitive::Stream (ref x) => x.serialize(out),
            Primitive::Dictionary (ref x) => x.serialize(out),
            Primitive::Array (ref x) => x.serialize(out),
            Primitive::Reference (ref x) => x.serialize(out),
            Primitive::Name (ref x) => x.serialize(out),
        }
    }
    fn from_primitive(p: Primitive, _: &Resolve) -> Result<Self> {
        Ok(p)
    }
}

impl<V: Object> Object for BTreeMap<String, V> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        match p {
            Primitive::Dictionary (dict) => {
                let mut new = Self::new();
                for (key, val) in dict.iter() {
                    new.insert(key.clone(), V::from_primitive(val.clone(), resolve)?);
                }
                Ok(new)
            }
            p =>  Err(ErrorKind::UnexpectedPrimitive {expected: "Dictionary", found: p.get_debug_name()}.into())
        }
    }
}

impl Object for () {
    fn serialize<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "null")

    }
    fn from_primitive(p: Primitive, resolve: &Resolve) -> Result<Self> {
        Ok(())
    }
}
