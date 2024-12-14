//! `Object` trait, along with some implementations. References.
//!
//! Some of the structs are incomplete (missing fields that are in the PDF references).

mod types;
mod stream;
mod color;
mod function;

pub use self::types::*;
pub use self::stream::*;
pub use self::color::*;
pub use self::function::*;
pub use crate::file::PromisedRef;
use crate::parser::ParseFlags;

use crate::primitive::*;
use crate::error::*;
use crate::enc::*;

use std::fmt;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::sync::Arc;
use std::ops::{Deref, Range};
use std::hash::{Hash, Hasher};
use std::convert::TryInto;
use datasize::DataSize;
use itertools::Itertools;

pub type ObjNr = u64;
pub type GenNr = u64;

pub struct ParseOptions {
    pub allow_error_in_option: bool,
    pub allow_xref_error: bool,
    pub allow_invalid_ops: bool,
    pub allow_missing_endobj: bool,
}
impl ParseOptions {
    pub const fn tolerant() -> Self {
        ParseOptions {
            allow_error_in_option: true,
            allow_xref_error: true,
            allow_invalid_ops: true,
            allow_missing_endobj: true,
        }
    }
    pub const fn strict() -> Self {
        ParseOptions {
            allow_error_in_option: false,
            allow_xref_error: false,
            allow_invalid_ops: true,
            allow_missing_endobj: false,
        }
    }
}

pub trait Resolve: {
    fn resolve_flags(&self, r: PlainRef, flags: ParseFlags, depth: usize) -> Result<Primitive>;
    fn resolve(&self, r: PlainRef) -> Result<Primitive> {
        self.resolve_flags(r, ParseFlags::ANY, 16)
    }
    fn get<T: Object+DataSize>(&self, r: Ref<T>) -> Result<RcRef<T>>;
    fn options(&self) -> &ParseOptions;
    fn stream_data(&self, id: PlainRef, range: Range<usize>) -> Result<Arc<[u8]>>;
    fn get_data_or_decode(&self, id: PlainRef, range: Range<usize>, filters: &[StreamFilter]) -> Result<Arc<[u8]>>;
}

pub struct NoResolve;
impl Resolve for NoResolve {
    fn resolve_flags(&self, _: PlainRef, _: ParseFlags, _: usize) -> Result<Primitive> {
        Err(PdfError::Reference)
    }
    fn get<T: Object+DataSize>(&self, _r: Ref<T>) -> Result<RcRef<T>> {
        Err(PdfError::Reference)
    }
    fn options(&self) -> &ParseOptions {
        static STRICT: ParseOptions = ParseOptions::strict();
        &STRICT
    }
    fn get_data_or_decode(&self, _: PlainRef, _: Range<usize>, _: &[StreamFilter]) -> Result<Arc<[u8]>> {
        Err(PdfError::Reference)
    }
    fn stream_data(&self, id: PlainRef, range: Range<usize>) -> Result<Arc<[u8]>> {
        Err(PdfError::Reference)
    }

}

/// A PDF Object
pub trait Object: Sized + Sync + Send + 'static {
    /// Convert primitive to Self
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self>;
}

pub trait Cloner: Updater + Resolve {
    fn clone_plainref(&mut self, old: PlainRef) -> Result<PlainRef>;
    fn clone_ref<T: DeepClone + Object + DataSize + ObjectWrite>(&mut self, old: Ref<T>) -> Result<Ref<T>>;
    fn clone_rcref<T: DeepClone + ObjectWrite + DataSize>(&mut self, old: &RcRef<T>) -> Result<RcRef<T>>;
    fn clone_shared<T: DeepClone>(&mut self, old: &Shared<T>) -> Result<Shared<T>>;
}

pub trait DeepClone: Sized + Sync + Send + 'static {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self>;
}

pub trait Updater {
    fn create<T: ObjectWrite>(&mut self, obj: T) -> Result<RcRef<T>>;
    fn update<T: ObjectWrite>(&mut self, old: PlainRef, obj: T) -> Result<RcRef<T>>;
    fn update_ref<T: ObjectWrite>(&mut self, old: &RcRef<T>, obj: T) -> Result<RcRef<T>> {
        self.update(old.get_ref().inner, obj)
    }
    fn promise<T: Object>(&mut self) -> PromisedRef<T>;
    fn fulfill<T: ObjectWrite>(&mut self, promise: PromisedRef<T>, obj: T) -> Result<RcRef<T>>;
}

pub struct NoUpdate;
impl Updater for NoUpdate {
    fn create<T: ObjectWrite>(&mut self, _obj: T) -> Result<RcRef<T>> { panic!() }
    fn update<T: ObjectWrite>(&mut self, _old: PlainRef, _obj: T) -> Result<RcRef<T>> { panic!() }
    fn promise<T: Object>(&mut self) -> PromisedRef<T> { panic!() }
    fn fulfill<T: ObjectWrite>(&mut self, _promise: PromisedRef<T>, _obj: T) -> Result<RcRef<T>> { panic!() }
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
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, DataSize)]
pub struct PlainRef {
    pub id:     ObjNr,
    pub gen:    GenNr,
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
impl DeepClone for PlainRef {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        cloner.clone_plainref(*self)
    }
}

// NOTE: Copy & Clone implemented manually ( https://github.com/rust-lang/rust/issues/26925 )

#[derive(DataSize)]
pub struct Ref<T> {
    inner:      PlainRef,
    _marker:    PhantomData<T>
}
impl<T> Clone for Ref<T> {
    fn clone(&self) -> Ref<T> {
        *self
    }
}
impl<T> Copy for Ref<T> {}

impl<T> Ref<T> {
    pub fn new(inner: PlainRef) -> Ref<T> {
        Ref {
            inner,
            _marker:    PhantomData,
        }
    }
    pub fn from_id(id: ObjNr) -> Ref<T> {
        Ref {
            inner:      PlainRef {id, gen: 0},
            _marker:    PhantomData,
        }
    }
    pub fn get_inner(&self) -> PlainRef {
        self.inner
    }
    pub fn upcast<U>(self) -> Ref<U> where T: SubType<U> {
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
impl<T: DeepClone+Object+DataSize+ObjectWrite> DeepClone for Ref<T> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        cloner.clone_ref(*self)
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

pub type Shared<T> = Arc<T>;


#[derive(Debug, DataSize)]
pub struct RcRef<T> {
    inner: PlainRef,
    data: Shared<T>
}
impl<T> From<RcRef<T>> for Primitive {
    fn from(value: RcRef<T>) -> Self {
        Primitive::Reference(value.inner)
    }
}
impl<T> From<RcRef<T>> for Ref<T> {
    fn from(value: RcRef<T>) -> Self {
        value.get_ref()
    }
}

impl<T> RcRef<T> {
    pub fn new(inner: PlainRef, data: Shared<T>) -> RcRef<T> {
        RcRef { inner, data }
    }
    pub fn get_ref(&self) -> Ref<T> {
        Ref::new(self.inner)
    }
    pub fn data(&self) -> &Shared<T> {
        &self.data
    }
}
impl<T: Object + std::fmt::Debug + DataSize> Object for RcRef<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(r) => resolve.get(Ref::new(r)),
            p => Err(PdfError::UnexpectedPrimitive {expected: "Reference", found: p.get_debug_name()})
        }
    }
}
impl<T> ObjectWrite for RcRef<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        self.inner.to_primitive(update)
    }
}
impl<T: DeepClone + std::fmt::Debug + DataSize + Object + ObjectWrite> DeepClone for RcRef<T> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        cloner.clone_rcref(self)
    }
}

impl<T> Deref for RcRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.data
    }
}
impl<T> Clone for RcRef<T> {
    fn clone(&self) -> RcRef<T> {
        RcRef {
            inner: self.inner,
            data: self.data.clone(),
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

#[derive(Debug, DataSize)]
pub enum MaybeRef<T> {
    Direct(Shared<T>),
    Indirect(RcRef<T>),
}
impl<T> MaybeRef<T> {
    pub fn as_ref(&self) -> Option<Ref<T>> {
        match *self {
            MaybeRef::Indirect(ref r) => Some(r.get_ref()),
            _ => None
        }
    }
    pub fn data(&self) -> &Shared<T> {
        match *self {
            MaybeRef::Direct(ref t) => t,
            MaybeRef::Indirect(ref r) => &r.data
        }
    }
}
impl<T: Object+DataSize> Object for MaybeRef<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        Ok(match p {
            Primitive::Reference(r) => MaybeRef::Indirect(resolve.get(Ref::new(r))?),
            p => MaybeRef::Direct(Shared::new(T::from_primitive(p, resolve)?))
        })
    }
}
impl<T: ObjectWrite> ObjectWrite for MaybeRef<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            MaybeRef::Direct(ref inner) => inner.to_primitive(update),
            MaybeRef::Indirect(r) => r.to_primitive(update)
        }
    }
}
impl<T: DeepClone + std::fmt::Debug + DataSize + Object + ObjectWrite> DeepClone for MaybeRef<T> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        match *self {
            MaybeRef::Direct(ref old) => cloner.clone_shared(old).map(MaybeRef::Direct),
            MaybeRef::Indirect(ref old) => cloner.clone_rcref(old).map(MaybeRef::Indirect)
        }
    }
}
impl<T> Deref for MaybeRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match *self {
            MaybeRef::Direct(ref t) => t,
            MaybeRef::Indirect(ref r) => r
        }
    }
}
impl<T> Clone for MaybeRef<T> {
    fn clone(&self) -> Self {
        match *self {
            MaybeRef::Direct(ref rc) => MaybeRef::Direct(rc.clone()),
            MaybeRef::Indirect(ref r) => MaybeRef::Indirect(r.clone())
        }
    }
}
impl<T> Trace for MaybeRef<T> {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        match *self {
            MaybeRef::Indirect(ref rc) => rc.trace(cb),
            MaybeRef::Direct(_) => ()
        }
    }
}
impl<T> From<Shared<T>> for MaybeRef<T> {
    fn from(r: Shared<T>) -> MaybeRef<T> {
        MaybeRef::Direct(r)
    }
}
impl<T> From<T> for MaybeRef<T> {
    fn from(t: T) -> MaybeRef<T> {
        MaybeRef::Direct(t.into())
    }
}
impl<T> From<MaybeRef<T>> for Shared<T> {
    fn from(r: MaybeRef<T>) -> Shared<T> {
        match r {
            MaybeRef::Direct(rc) => rc,
            MaybeRef::Indirect(r) => r.data
        }
    }
}
impl<'a, T> From<&'a MaybeRef<T>> for Shared<T> {
    fn from(r: &'a MaybeRef<T>) -> Shared<T> {
        match r {
            MaybeRef::Direct(ref rc) => rc.clone(),
            MaybeRef::Indirect(ref r) => r.data.clone()
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

#[derive(Debug, Clone, DataSize)]
pub struct Lazy<T> {
    primitive: Primitive,
    _marker: PhantomData<T>
}
impl<T: Object> Lazy<T> {
    pub fn load(&self, resolve: &impl Resolve) -> Result<T> {
        T::from_primitive(self.primitive.clone(), resolve)
    }
    pub fn safe(value: T, update: &mut impl Updater) -> Result<Self>
    where T: ObjectWrite
    {
        let primitive = value.to_primitive(update)?;
        Ok(Lazy { primitive, _marker: PhantomData })
    }
}
impl<T: Object> Object for Lazy<T> {
    fn from_primitive(p: Primitive, _: &impl Resolve) -> Result<Self> {
        Ok(Self { primitive: p, _marker: PhantomData })
    }
}
impl<T: ObjectWrite> ObjectWrite for Lazy<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        Ok(self.primitive.clone())
    }
}
impl<T> Default for Lazy<T> {
    fn default() -> Self {
        Lazy { primitive: Primitive::Null, _marker: PhantomData }
    }
}

impl<T> From<RcRef<T>> for Lazy<T> {
    fn from(value: RcRef<T>) -> Self {
        Lazy { primitive: Primitive::Reference(value.inner), _marker: PhantomData }
    }
}

//////////////////////////////////////
// Object for Primitives & other types
//////////////////////////////////////

impl Object for i32 {
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Reference(id) => r.resolve(id)?.as_integer(),
            p => p.as_integer()
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
            p => p.as_u32()
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
            p => Ok(p.as_u32()? as usize)
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
            p => p.as_number()
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
            p => p.as_bool()
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
            _ => Err(PdfError::UnexpectedPrimitive {expected: "Dictionary", found: p.get_debug_name()}),
        }
    }
}

impl Object for Name {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        p.resolve(resolve)?.into_name()
    }
}
impl ObjectWrite for Name {
    fn to_primitive(&self, _: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Name(self.0.clone()))
    }
}

impl<T: Object> Object for Vec<T> {
    /// Will try to convert `p` to `T` first, then try to convert `p` to Vec<T>
    fn from_primitive(p: Primitive, r: &impl Resolve) -> Result<Self> {
        Ok(
        match p {
            Primitive::Array(_) => {
                p.resolve(r)?.into_array()?
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
impl<T: ObjectWrite> ObjectWrite for Vec<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        Primitive::array::<T, _, _, _>(self.iter(), update)
    }
}
impl<T: DeepClone> DeepClone for Vec<T> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        self.iter().map(|t| t.deep_clone(cloner)).collect()
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
impl DeepClone for Primitive {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        match *self {
            Primitive::Array(ref parts) => Ok(Primitive::Array(parts.into_iter().map(|p| p.deep_clone(cloner)).try_collect()?)),
            Primitive::Boolean(b) => Ok(Primitive::Boolean(b)),
            Primitive::Dictionary(ref dict) => Ok(Primitive::Dictionary(dict.deep_clone(cloner)?)),
            Primitive::Integer(i) => Ok(Primitive::Integer(i)),
            Primitive::Name(ref name) => Ok(Primitive::Name(name.clone())),
            Primitive::Null => Ok(Primitive::Null),
            Primitive::Number(n) => Ok(Primitive::Number(n)),
            Primitive::Reference(r) => Ok(Primitive::Reference(r.deep_clone(cloner)?)),
            Primitive::Stream(ref s) => Ok(Primitive::Stream(s.deep_clone(cloner)?)),
            Primitive::String(ref s) => Ok(Primitive::String(s.clone()))
        }
    }
}

impl Trace for Primitive {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        match *self {
            Primitive::Reference(r) => cb(r),
            Primitive::Array(ref parts) => parts.iter().for_each(|p| p.trace(cb)),
            Primitive::Dictionary(ref dict) => dict.values().for_each(|p| p.trace(cb)),
            _ => ()
        }
    }
}

impl<V: Object> Object for HashMap<Name, V> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Null => Ok(HashMap::new()),
            Primitive::Dictionary (dict) => {
                let mut new = Self::new();
                for (key, val) in dict.iter() {
                    new.insert(key.clone(), V::from_primitive(val.clone(), resolve)?);
                }
                Ok(new)
            }
            Primitive::Reference (id) => HashMap::from_primitive(resolve.resolve(id)?, resolve),
            p => Err(PdfError::UnexpectedPrimitive {expected: "Dictionary", found: p.get_debug_name()})
        }
    }
}
impl<V: ObjectWrite> ObjectWrite for HashMap<Name, V> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        if self.is_empty() {
            Ok(Primitive::Null)
        } else {
            let mut dict = Dictionary::new();
            for (k, v) in self.iter() {
                dict.insert(k.clone(), v.to_primitive(update)?);
            }
            Ok(Primitive::Dictionary(dict))
        }
    }
}
impl<V: DeepClone> DeepClone for HashMap<Name, V> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        self.iter().map(|(k, v)| Ok((k.clone(), v.deep_clone(cloner)?))).collect()
    }
}

impl<T: Object> Object for Option<T> {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        match p {
            Primitive::Null => Ok(None),
            p => match T::from_primitive(p, resolve) {
                Ok(p) => Ok(Some(p)),
                // References to non-existing objects ought not to be an error
                Err(PdfError::NullRef {..}) => Ok(None),
                Err(PdfError::FreeObject {..}) => Ok(None),
                Err(e) if resolve.options().allow_error_in_option => {
                    warn!("ignoring {:?}", e);
                    Ok(None)
                }
                Err(e) => Err(e)
            }
        }
    }
}
impl<T: ObjectWrite> ObjectWrite for Option<T> {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        match self {
            None => Ok(Primitive::Null),
            Some(t) => t.to_primitive(update)
        }
    }
}
impl<T: DeepClone> DeepClone for Option<T> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        match self {
            None => Ok(None),
            Some(t) => t.deep_clone(cloner).map(Some)
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

impl<T, U> Object for (T, U) where T: Object, U: Object {
    fn from_primitive(p: Primitive, resolve: &impl Resolve) -> Result<Self> {
        let arr = p.resolve(resolve)?.into_array()?;
        if arr.len() != 2 {
            bail!("expected array of length 2 (found {})", arr.len());
        }
        let [a, b]: [Primitive; 2] = arr.try_into().unwrap();
        Ok((T::from_primitive(a, resolve)?, U::from_primitive(b, resolve)?))
    }
}

impl<T, U> ObjectWrite for (T, U) where T: ObjectWrite, U: ObjectWrite {
    fn to_primitive(&self, update: &mut impl Updater) -> Result<Primitive> {
        Ok(Primitive::Array(vec![self.0.to_primitive(update)?, self.1.to_primitive(update)?]))
    }
}

impl<T: Trace, U: Trace> Trace for (T, U) {
    fn trace(&self, cb: &mut impl FnMut(PlainRef)) {
        self.0.trace(cb);
        self.1.trace(cb);
    }
}

impl<T: DeepClone> DeepClone for Box<T> {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        Ok(Box::new((&**self).deep_clone(cloner)?))
    }
}
macro_rules! deep_clone_simple {
    ($($t:ty),*) => (
        $(
            impl DeepClone for $t {
                fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
                    Ok(self.clone())
                }
            }
        )*
    )
}
deep_clone_simple!(f32, i32, u32, bool, Name, (), Date, PdfString, Rectangle, u8, Arc<[u8]>, Vec<u16>);

impl<A: DeepClone, B: DeepClone> DeepClone for (A, B) {
    fn deep_clone(&self, cloner: &mut impl Cloner) -> Result<Self> {
        Ok((self.0.deep_clone(cloner)?, self.1.deep_clone(cloner)?))
    }
}
