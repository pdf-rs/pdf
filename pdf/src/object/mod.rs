//! `Object` trait, along with some implementations. References.
//!
//! Some of the structs are incomplete (missing fields that are in the PDF references).

mod color;
mod function;
mod stream;
mod types;

pub use self::color::*;
pub use self::function::*;
pub use self::stream::*;
pub use self::types::*;
pub use crate::file::PromisedRef;

use crate::enc::*;
use crate::error::*;
use crate::primitive::*;

use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;

pub type ObjNr = u64;
pub type GenNr = u16;

pub trait Resolve {
    fn resolve(&self, r: PlainRef) -> Result<Primitive>;
    fn get<T: Object>(&self, r: Ref<T>) -> Result<RcRef<T>>;
}

pub struct NoResolve;
impl Resolve for NoResolve {
    fn resolve(&self, _: PlainRef) -> Result<Primitive> {
        Err(PdfError::Reference)
    }
    fn get<T: Object>(&self, _r: Ref<T>) -> Result<RcRef<T>> {
        Err(PdfError::Reference)
    }
}

/// A PDF Object
pub trait Object: Sized + 'static {
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self>;
}

pub trait Updater {
    fn create<T: ObjectWrite>(&mut self, obj: T) -> Result<RcRef<T>>;
    fn update<T: ObjectWrite>(&mut self, old: PlainRef, obj: T) -> Result<RcRef<T>>;
    fn promise<T: Object>(&mut self) -> PromisedRef<T>;
    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>>;
}

pub struct NoUpdate;
impl Updater for NoUpdate {
    fn create<T: ObjectWrite>(&mut self, _obj: T) -> Result<RcRef<T>> {
        panic!()
    }
    fn update<T: ObjectWrite>(&mut self, _old: PlainRef, _obj: T) -> Result<RcRef<T>> {
        panic!()
    }
    fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        panic!()
    }
    fn fulfill<T: ObjectWrite>(&mut self, _promise: PromisedRef<T>, _obj: T) -> Result<RcRef<T>> {
        panic!()
    }
}

pub trait ObjectWrite {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive>;
}

pub trait FromDict: Sized {
    fn from_dict(dict: Dictionary, resolve: &impl Resolve) -> Result<Self>;
}
pub trait ToDict: ObjectWrite {
    fn to_dict(&self, update: &mut impl Updater) -> Result<Dictionary>;
}

pub trait SubType<T> {}

pub trait Trace {
    fn trace(&self, _cb: &mut impl FnMut(PlainRef)) {}
}

///////
// Refs
///////

// TODO move to primitive.rs
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PlainRef {
    pub id:  ObjNr,
    pub gen: GenNr,
}
impl Object for PlainRef {
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        p.into_reference()
    }
}
impl ObjectWrite for PlainRef {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Reference(*self))
    }
}

// NOTE: Copy & Clone implemented manually ( https://github.com/rust-lang/rust/issues/26925 )

pub struct Ref<T> {
    inner:   PlainRef,
    _marker: PhantomData<T>,
}
impl<T> Clone for Ref<T> {
    fn clone(&self) -> Ref<T> {
        Ref {
            inner:   self.inner,
            _marker: PhantomData,
        }
    }
}
impl<T> Copy for Ref<T> {}

impl<T> Ref<T> {
    pub fn new(inner: PlainRef) -> Ref<T> {
        Ref {
            inner,
            _marker: PhantomData::default(),
        }
    }
    pub fn from_id(id: ObjNr) -> Ref<T> {
        Ref {
            inner:   PlainRef { id, gen: 0 },
            _marker: PhantomData::default(),
        }
    }
    pub fn get_inner(&self) -> PlainRef {
        self.inner
    }
    pub fn upcast<U>(self) -> Ref<U>
    where
        T: SubType<U>,
    {
        Ref::new(self.inner)
    }
}
impl<T: Object> Object for Ref<T> {
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        Ok(Ref::new(p.into_reference()?))
    }
}
impl<T> ObjectWrite for Ref<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.inner.to_primitive(update)
    }
}
impl<T> Trace for Ref<T> {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        cb(self.inner);
    }
}
impl<T> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ref({})", self.inner.id)
    }
}
impl<T> Hash for Ref<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state)
    }
}
impl<T> PartialEq for Ref<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.inner.eq(&rhs.inner)
    }
}
impl<T> Eq for Ref<T> {}

#[derive(Debug)]
pub struct RcRef<T> {
    inner: PlainRef,
    data:  Rc<T>,
}

impl<T> RcRef<T> {
    pub fn new(inner: PlainRef, data: Rc<T>) -> RcRef<T> {
        RcRef { inner, data }
    }
    pub fn get_ref(&self) -> Ref<T> {
        Ref::new(self.inner)
    }
}
impl<T: Object + std::fmt::Debug> Object for RcRef<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(r) => resolve.get(Ref::new(r)),
            p => Err(PdfError::UnexpectedPrimitive {
                expected: "Reference",
                found:    p.get_debug_name(),
            }),
        }
    }
}
impl<T> ObjectWrite for RcRef<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.inner.to_primitive(update)
    }
}
impl<T> Deref for RcRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.data
    }
}
impl<T> Clone for RcRef<T> {
    fn clone(&self) -> RcRef<T> {
        RcRef {
            inner: self.inner,
            data:  self.data.clone(),
        }
    }
}
impl<T> Trace for RcRef<T> {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        cb(self.inner);
    }
}
impl<'a, T> From<&'a RcRef<T>> for Ref<T> {
    fn from(r: &'a RcRef<T>) -> Ref<T> {
        Ref::new(r.inner)
    }
}
impl<T> Hash for RcRef<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(&**self, state)
    }
}
impl<T> PartialEq for RcRef<T> {
    fn eq(&self, rhs: &Self) -> bool {
        std::ptr::eq(&**self, &**rhs)
    }
}
impl<T> Eq for RcRef<T> {}

#[derive(Debug)]
pub enum MaybeRef<T> {
    Direct(Rc<T>),
    Indirect(RcRef<T>),
}
impl<T> MaybeRef<T> {
    pub fn as_ref(&self) -> Option<Ref<T>> {
        match *self {
            MaybeRef::Indirect(ref r) => Some(r.get_ref()),
            _ => None,
        }
    }
}
impl<T: Object> Object for MaybeRef<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        Ok(match p {
            Primitive::Reference(r) => MaybeRef::Indirect(resolve.get(Ref::new(r))?),
            p => MaybeRef::Direct(Rc::new(T::from_primitive(p, resolve)?)),
        })
    }
}
impl<T: ObjectWrite> ObjectWrite for MaybeRef<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            MaybeRef::Direct(ref inner) => inner.to_primitive(update),
            MaybeRef::Indirect(r) => r.to_primitive(update),
        }
    }
}
impl<T> Deref for MaybeRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match *self {
            MaybeRef::Direct(ref t) => t,
            MaybeRef::Indirect(ref r) => &**r,
        }
    }
}
impl<T> Clone for MaybeRef<T> {
    fn clone(&self) -> Self {
        match *self {
            MaybeRef::Direct(ref rc) => MaybeRef::Direct(rc.clone()),
            MaybeRef::Indirect(ref r) => MaybeRef::Indirect(r.clone()),
        }
    }
}
impl<T> Trace for MaybeRef<T> {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        match *self {
            MaybeRef::Indirect(ref rc) => rc.trace(cb),
            MaybeRef::Direct(_) => (),
        }
    }
}
impl<T> From<Rc<T>> for MaybeRef<T> {
    fn from(r: Rc<T>) -> MaybeRef<T> {
        MaybeRef::Direct(r)
    }
}
impl<T> From<MaybeRef<T>> for Rc<T> {
    fn from(r: MaybeRef<T>) -> Rc<T> {
        match r {
            MaybeRef::Direct(rc) => rc,
            MaybeRef::Indirect(r) => r.data,
        }
    }
}
impl<'a, T> From<&'a MaybeRef<T>> for Rc<T> {
    fn from(r: &'a MaybeRef<T>) -> Rc<T> {
        match r {
            MaybeRef::Direct(ref rc) => rc.clone(),
            MaybeRef::Indirect(ref r) => r.data.clone(),
        }
    }
}
impl<T> From<RcRef<T>> for MaybeRef<T> {
    fn from(r: RcRef<T>) -> MaybeRef<T> {
        MaybeRef::Indirect(r)
    }
}
impl<T> Hash for MaybeRef<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(&**self, state)
    }
}
impl<T> PartialEq for MaybeRef<T> {
    fn eq(&self, rhs: &Self) -> bool {
        std::ptr::eq(&**self, &**rhs)
    }
}
impl<T> Eq for MaybeRef<T> {}

//////////////////////////////////////
// Object for Primitives & other types
//////////////////////////////////////

impl Object for i32 {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(id) => r.resolve(id)?.as_integer(),
            p => p.as_integer(),
        }
    }
}
impl ObjectWrite for i32 {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Integer(*self))
    }
}

impl Object for u32 {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(id) => r.resolve(id)?.as_u32(),
            p => p.as_u32(),
        }
    }
}
impl ObjectWrite for u32 {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Integer(*self as _))
    }
}

impl Object for usize {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(id) => Ok(r.resolve(id)?.as_u32()? as usize),
            p => Ok(p.as_u32()? as usize),
        }
    }
}
impl ObjectWrite for usize {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Integer(*self as _))
    }
}

impl Object for f32 {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(id) => r.resolve(id)?.as_number(),
            p => p.as_number(),
        }
    }
}
impl ObjectWrite for f32 {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Number(*self))
    }
}

impl Object for bool {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(id) => r.resolve(id)?.as_bool(),
            p => p.as_bool(),
        }
    }
}
impl ObjectWrite for bool {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Boolean(*self))
    }
}

impl Object for Dictionary {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Dictionary(dict) => Ok(dict),
            Primitive::Reference(id) => Dictionary::from_primitive(r.resolve(id)?, r),
            _ => Err(PdfError::UnexpectedPrimitive {
                expected: "Dictionary",
                found:    p.get_debug_name(),
            }),
        }
    }
}

impl Object for String {
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        p.into_name()
    }
}

impl<T: Object> Object for Vec<T> {
    /// Will try to convert `p` to `T` first, then try to convert `p` to Vec<T>
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        Ok(match p {
            Primitive::Array(_) => p
                .into_array(r)?
                .into_iter()
                .map(|p| T::from_primitive(p, r))
                .collect::<Result<Vec<T>>>()?,
            Primitive::Null => Vec::new(),
            Primitive::Reference(id) => Self::from_primitive(r.resolve(id)?, r)?,
            _ => vec![T::from_primitive(p, r)?],
        })
    }
}
impl<T: ObjectWrite> ObjectWrite for Vec<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        Primitive::array::<T, _, _, _>(self.iter(), update)
    }
}
impl<T: Trace> Trace for Vec<T> {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        for i in self.iter() {
            i.trace(cb);
        }
    }
}
/*
pub struct Data(pub Vec<u8>);
impl Object for Data {
    fn serialize<W: io::Write>(&self, out: &mut W) -> Result<()> {
        unimplemented!()
    }
    /// Will try to convert `p` to `T` first, then try to convert `p` to Vec<T>
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Array(_) => {
                p.into_array(r)?
                    .into_iter()
                    .map(|p| u8::from_primitive(p, r))
                    .collect::<Result<Vec<T>>>()?
            },
            Primitive::Null => {
                Vec::new()
            }
            Primitive::Reference(id) => Self::from_primitive(r.resolve(id)?, r)?,
            _ =>
        }
    }
}*/

impl Object for Primitive {
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        Ok(p)
    }
}
impl ObjectWrite for Primitive {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(self.clone())
    }
}
impl Trace for Primitive {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        match *self {
            Primitive::Reference(r) => cb(r),
            Primitive::Array(ref parts) => parts.iter().for_each(|p| p.trace(cb)),
            Primitive::Dictionary(ref dict) => dict.values().for_each(|p| p.trace(cb)),
            _ => (),
        }
    }
}

impl ObjectWrite for String {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Name(self.clone()))
    }
}
impl<V: Object> Object for HashMap<String, V> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Null => Ok(HashMap::new()),
            Primitive::Dictionary(dict) => {
                let mut new = Self::new();
                for (key, val) in dict.iter() {
                    new.insert(key.clone(), V::from_primitive(val.clone(), resolve)?);
                }
                Ok(new)
            }
            Primitive::Reference(id) => HashMap::from_primitive(resolve.resolve(id)?, resolve),
            p => Err(PdfError::UnexpectedPrimitive {
                expected: "Dictionary",
                found:    p.get_debug_name(),
            }),
        }
    }
}
impl<V: ObjectWrite> ObjectWrite for HashMap<String, V> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        if self.is_empty() {
            Ok(Primitive::Null)
        } else {
            let mut dict = Dictionary::new();
            for (k, v) in self.iter() {
                dict.insert(k, v.to_primitive(update)?);
            }
            Ok(Primitive::Dictionary(dict))
        }
    }
}

impl<T: Object> Object for Option<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Null => Ok(None),
            p => match T::from_primitive(p, resolve) {
                Ok(p) => Ok(Some(p)),
                // References to non-existing objects ought not to be an error
                Err(PdfError::NullRef { .. }) => Ok(None),
                Err(PdfError::FreeObject { .. }) => Ok(None),
                Err(e) => Err(e),
            },
        }
    }
}
impl<T: ObjectWrite> ObjectWrite for Option<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            None => Ok(Primitive::Null),
            Some(t) => t.to_primitive(update),
        }
    }
}
impl<T: Trace> Trace for Option<T> {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        if let Some(ref t) = *self {
            t.trace(cb)
        }
    }
}

impl<T: Object> Object for Box<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        T::from_primitive(p, resolve).map(Box::new)
    }
}
impl<T: ObjectWrite> ObjectWrite for Box<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        (**self).to_primitive(update)
    }
}
impl<T: Trace> Trace for Box<T> {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        (**self).trace(cb)
    }
}

impl Object for () {
    fn from_primitive(_p: Primitive, _resolve: &impl Resolve) -> Result<Self> {
        Ok(())
    }
}
impl ObjectWrite for () {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Null)
    }
}
impl Trace for () {}

impl<T, U> Object for (T, U)
where
    T: Object,
    U: Object,
{
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let arr = p.into_array(resolve)?;
        if arr.len() != 2 {
            bail!("expected array of length 2 (found {})", arr.len());
        }
        let [a, b]: [Primitive; 2] = arr.try_into().unwrap();
        Ok((
            T::from_primitive(a, resolve)?,
            U::from_primitive(b, resolve)?,
        ))
    }
}

impl<T, U> ObjectWrite for (T, U)
where
    T: ObjectWrite,
    U: ObjectWrite,
{
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Array(vec![
            self.0.to_primitive(update)?,
            self.1.to_primitive(update)?,
        ]))
    }
}

impl<T: Trace, U: Trace> Trace for (T, U) {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        self.0.trace(cb);
        self.1.trace(cb);
    }
}
