//! `Object` trait, along with some implementations. References.
//!
//! Some of the structs are incomplete (missing fields that are in the PDF references).

mod types;
mod stream;

pub use self::types::*;
pub use self::stream::*;

use crate::primitive::*;
use crate::error::*;
use crate::enc::*;

use std::io;
use std::fmt;
use std::marker::PhantomData;
use std::collections::BTreeMap;
use std::rc::Rc;

pub type ObjNr = u64;
pub type GenNr = u16;

pub trait Resolve: {
    fn resolve(&self, r: PlainRef) -> Result<Primitive>;
    fn get<T: Object>(&self, r: Ref<T>) -> Result<Rc<T>>;
}

pub struct NoResolve;
impl Resolve for NoResolve {
    fn resolve(&self, _: PlainRef) -> Result<Primitive> {
        Err(PdfError::Reference)
    }
    fn get<T: Object>(&self, r: Ref<T>) -> Result<Rc<T>> {
        Err(PdfError::Reference)
    }
}

/// A PDF Object
pub trait Object: Sized + 'static {
    /// Write object as a byte stream
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()>;
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self>;
    
    fn from_dict(dict: Dictionary, resolve: &impl Resolve) -> Result<Self> {
        Self::from_primitive(Primitive::Dictionary(dict), resolve)
    }
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
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()>  {
        write!(out, "{} {} R", self.id, self.gen)?;
        Ok(())
    }
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        p.to_reference()
    }
}


// NOTE: Copy & Clone implemented manually ( https://github.com/rust-lang/rust/issues/26925 )

pub struct Ref<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}
impl<T> Clone for Ref<T> {
    fn clone(&self) -> Ref<T> {
        Ref {
            inner: self.inner,
            _marker: PhantomData
        }
    }
}
impl<T> Copy for Ref<T> {}

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
impl<T: Object> Ref<T> {
    pub fn resolve(&self, r: &impl Resolve) -> Result<T> {
        T::from_primitive(r.resolve(self.inner)?, r)
    }
}
impl<T: Object> Object for Ref<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()>  {
        self.inner.serialize(out)
    }
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
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
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "{}", self)?;
        Ok(())
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Integer (n) => Ok(n),
            Primitive::Reference (r) => Ok(i32::from_primitive(resolve.resolve(r)?, resolve)?),
            p => Err(PdfError::UnexpectedPrimitive {expected: "Integer", found: p.get_debug_name()})
        }
    }
}
impl Object for u32 {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "{}", self)?;
        Ok(())
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Integer (n) => Ok(n as u32),
            Primitive::Reference (r) => Ok(u32::from_primitive(resolve.resolve(r)?, resolve)?),
            p => Err(PdfError::UnexpectedPrimitive {expected: "Integer", found: p.get_debug_name()})
        }
    }
}
impl Object for usize {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "{}", self)?;
        Ok(())
    }
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        Ok(i32::from_primitive(p, r)? as usize)
    }
}
impl Object for f32 {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "{}", self)?;
        Ok(())
    }
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        p.as_number()
    }
}
impl Object for bool {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "{}", self)?;
        Ok(())
    }
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        p.as_bool()
    }
}
impl Object for Dictionary {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "<<")?;
        for (key, val) in self.iter() {
            write!(out, "/{} ", key)?;
            val.serialize(out)?;
        }
        write!(out, ">>")?;
        Ok(())
    }
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Dictionary(dict) => Ok(dict),
            Primitive::Reference(id) => Dictionary::from_primitive(r.resolve(id)?, r),
            _ => Err(PdfError::UnexpectedPrimitive {expected: "Dictionary", found: p.get_debug_name()}),
        }
    }
}

impl Object for String {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
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
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        Ok(p.to_name()?)
    }
}

impl<T: Object> Object for Vec<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write_list(out, self.iter())
    }
    /// Will try to convert `p` to `T` first, then try to convert `p` to Vec<T>
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
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
            Primitive::Reference(id) => Self::from_primitive(r.resolve(id)?, r)?,
            _ => vec![T::from_primitive(p, r)?]
        }
        )
    }
}

impl Object for Primitive {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        match *self {
            Primitive::Null => write!(out, "null")?,
            Primitive::Integer (ref x) => x.serialize(out)?,
            Primitive::Number (ref x) => x.serialize(out)?,
            Primitive::Boolean (ref x) => x.serialize(out)?,
            Primitive::String (ref x) => x.serialize(out)?,
            Primitive::Stream (ref x) => x.serialize(out)?,
            Primitive::Dictionary (ref x) => x.serialize(out)?,
            Primitive::Array (ref x) => x.serialize(out)?,
            Primitive::Reference (ref x) => x.serialize(out)?,
            Primitive::Name (ref x) => x.serialize(out)?,
        }
        Ok(())
    }
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        Ok(p)
    }
}

impl<V: Object> Object for BTreeMap<String, V> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Null => Ok(BTreeMap::new()),
            Primitive::Dictionary (dict) => {
                let mut new = Self::new();
                for (key, val) in dict.iter() {
                    new.insert(key.clone(), V::from_primitive(val.clone(), resolve)?);
                }
                Ok(new)
            }
            Primitive::Reference (id) => BTreeMap::from_primitive(resolve.resolve(id)?, resolve),
            p =>  Err(PdfError::UnexpectedPrimitive {expected: "Dictionary", found: p.get_debug_name()}.into())
        }
    }
}

impl<T: Object + std::fmt::Debug> Object for Rc<T> {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        (**self).serialize(out)
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(r) => resolve.get(Ref::new(r)),
            p => Ok(Rc::new(T::from_primitive(p, resolve)?))
        }
    }
}

impl<T: Object> Object for Option<T> {
    fn serialize<W: io::Write>(&self, _out: &mut W) -> Result<()> {
        // TODO: the Option here is most often or always about whether the entry exists in a
        // dictionary. Hence it should probably be more up to the Dictionary impl of serialize, to
        // handle Options. 
        unimplemented!();
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Null => Ok(None),
            p => match T::from_primitive(p, resolve) {
                Ok(p) => Ok(Some(p)),
                // References to non-existing objects ought not to be an error
                Err(PdfError::NullRef {..}) => Ok(None),
                Err(PdfError::FreeObject {..}) => Ok(None),
                Err(e) => Err(e),
            }
        }
    }
}

impl Object for () {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "null")?;
        Ok(())
    }
    fn from_primitive(_p: Primitive, _resolve: &impl Resolve) -> Result<Self> {
        Ok(())
    }
}

impl<T, U> Object for (T, U) where T: Object, U: Object {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        write!(out, "[")?;
        self.0.serialize(out)?;
        write!(out, " ")?;
        self.1.serialize(out)?;
        write!(out, "]")?;
        Ok(())
    }
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let mut arr = p.to_array(resolve)?;
        if arr.len() != 2 {
            bail!("expected array of length 2 (found {})", arr.len());
        }
        let b = arr.pop().unwrap();
        let a = arr.pop().unwrap();
        Ok((T::from_primitive(a, resolve)?, U::from_primitive(b, resolve)?))
    }
}
