use crate::error::{PdfError, Result};
use datasize::DataSize;
use std::any::TypeId;
use std::rc::Rc;
use std::sync::Arc;

pub trait AnyObject {
    fn type_name(&self) -> &'static str;
    fn type_id(&self) -> TypeId;
    fn size(&self) -> usize;
}

#[repr(transparent)]
pub struct NoSize<T>(T);
impl<T: 'static> AnyObject for NoSize<T> {
    fn size(&self) -> usize {
        0
    }
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

#[repr(transparent)]
pub struct WithSize<T>(T);
impl<T: DataSize + 'static> AnyObject for WithSize<T> {
    fn size(&self) -> usize {
        datasize::data_size(&self.0)
    }
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

#[derive(DataSize)]
pub struct Any(Rc<dyn AnyObject>);

impl Any {
    pub fn downcast<T>(self) -> Result<Rc<T>>
    where
        T: AnyObject + 'static,
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
    where
        WithSize<T>: AnyObject,
        T: 'static,
    {
        Any(unsafe { std::mem::transmute::<Rc<T>, Rc<WithSize<T>>>(rc) } as _)
    }
    pub fn new_without_size<T>(rc: Rc<T>) -> Any
    where
        NoSize<T>: AnyObject,
        T: 'static,
    {
        Any(unsafe { std::mem::transmute::<Rc<T>, Rc<NoSize<T>>>(rc) } as _)
    }
    pub fn type_name(&self) -> &'static str {
        self.0.type_name()
    }
}

#[derive(Clone, DataSize)]
pub struct AnySync(Arc<dyn AnyObject + Sync + Send>);

#[cfg(feature = "cache")]
impl globalcache::ValueSize for AnySync {
    #[inline]
    fn size(&self) -> usize {
        self.0.size()
    }
}

impl AnySync {
    pub fn downcast<T>(self) -> Result<Arc<T>>
    where
        T: 'static,
    {
        if TypeId::of::<T>() == self.0.type_id() {
            unsafe {
                let raw: *const (dyn AnyObject + Sync + Send) = Arc::into_raw(self.0);
                Ok(Arc::from_raw(raw as *const T))
            }
        } else {
            Err(type_mismatch::<T>(self.0.type_name()))
        }
    }
    pub fn new<T>(arc: Arc<T>) -> AnySync
    where
        WithSize<T>: AnyObject,
        T: Sync + Send + 'static,
    {
        AnySync(unsafe { std::mem::transmute::<Arc<T>, Arc<WithSize<T>>>(arc) } as _)
    }
    pub fn new_without_size<T>(arc: Arc<T>) -> AnySync
    where
        NoSize<T>: AnyObject,
        T: Sync + Send + 'static,
    {
        AnySync(unsafe { std::mem::transmute::<Arc<T>, Arc<NoSize<T>>>(arc) } as _)
    }
    pub fn type_name(&self) -> &'static str {
        self.0.type_name()
    }
}
fn type_mismatch<T>(name: &str) -> PdfError {
    PdfError::Other {
        msg: format!("expected {}, found {}", std::any::type_name::<T>(), name),
    }
}
