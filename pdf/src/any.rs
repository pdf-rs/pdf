use std::any::TypeId;
use std::rc::Rc;
use std::sync::Arc;
use datasize::DataSize;
use crate::object::{Object};
use crate::error::{Result, PdfError};

pub trait AnyObject {
    fn type_name(&self) -> &'static str;
    fn type_id(&self) -> TypeId;
    fn size(&self) -> usize;
}
impl<T> AnyObject for T
    where T: Object + 'static + DataSize
{
    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
    fn size(&self) -> usize {
        datasize::data_size(self)
    }
}

#[derive(DataSize)]
pub struct Any(Rc<dyn AnyObject>);

impl Any {
    pub fn downcast<T>(self) -> Result<Rc<T>> 
        where T: AnyObject + 'static
    {
        if TypeId::of::<T>() == self.0.type_id() {
            unsafe {
                let raw: *const dyn AnyObject = Rc::into_raw(self.0);
                Ok(Rc::from_raw(raw as *const T))
            }
        } else {
            Err(type_mismatch::<T>(self.0.type_name()))
        }
    }
    pub fn new<T>(rc: Rc<T>) -> Any
        where T: AnyObject + 'static
    {
        Any(rc as _)
    }
    pub fn type_name(&self) -> &'static str {
        self.0.type_name()
    }
}
impl<T: AnyObject + 'static> From<Rc<T>> for Any {
    fn from(t: Rc<T>) -> Self {
        Any::new(t)
    }
}

#[derive(Clone, DataSize)]
pub struct AnySync(Arc<dyn AnyObject+Sync+Send>);

#[cfg(feature="cache")]
impl globalcache::ValueSize for AnySync {
    #[inline]
    fn size(&self) -> usize {
        self.0.size()
    }
}

impl AnySync {
    pub fn downcast<T>(self) -> Result<Arc<T>> 
        where T: AnyObject + Sync + Send + 'static
    {
        if TypeId::of::<T>() == self.0.type_id() {
            unsafe {
                let raw: *const (dyn AnyObject+Sync+Send) = Arc::into_raw(self.0);
                Ok(Arc::from_raw(raw as *const T))
            }
        } else {
            Err(type_mismatch::<T>(self.0.type_name()))
        }
    }
    pub fn new<T>(arc: Arc<T>) -> AnySync
        where T: AnyObject + Sync + Send + 'static
    {
        AnySync(arc as _)
    }
    pub fn type_name(&self) -> &'static str {
        self.0.type_name()
    }
}
impl<T: AnyObject + Sync + Send + 'static> From<Arc<T>> for AnySync {
    fn from(t: Arc<T>) -> Self {
        AnySync::new(t)
    }
}
fn type_mismatch<T: AnyObject + 'static>(name: &str) -> PdfError {
    PdfError::Other { msg: format!("expected {}, found {}", std::any::type_name::<T>(), name) }
}
