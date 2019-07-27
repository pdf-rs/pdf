use std::any::TypeId;
use std::rc::Rc;
use crate::object::Object;
use crate::error::{Result, PdfError};

pub trait AnyObject {
    fn serialize(&self, out: &mut Vec<u8>);
    #[cfg(feature="nightly")]
    fn type_name(&self) -> &'static str;
    fn type_id(&self) -> TypeId;
}
impl<T> AnyObject for T
    where T: Object + 'static
{
    fn serialize(&self, out: &mut Vec<u8>) {
        Object::serialize(self, out).expect("write error on Vec<u8> ?!?")
    }
    #[cfg(feature="nightly")]
    fn type_name(&self) -> &'static str {
        unsafe {
            std::intrinsics::type_name::<T>()
        }
    }
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

#[derive(Clone)]
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
            Err(type_mismatch::<T>(&self))
        }
    }
    pub fn new<T>(rc: Rc<T>) -> Any
        where T: AnyObject + 'static
    {
        Any(rc as _)
    }
    #[cfg(feature="nightly")]
    pub fn type_name(&self) -> &'static str {
        self.0.type_name()
    }
}

#[cfg(feature="nightly")]
fn type_mismatch<T: AnyObject + 'static>(any: &Any) -> PdfError {
    PdfError::Other { msg: format!("expected {}, found {}", unsafe { std::intrinsics::type_name::<T>() }, any.type_name()) }
}
#[cfg(not(feature="nightly"))]
fn type_mismatch<T: AnyObject + 'static>(any: &Any) -> PdfError {
    PdfError::Other { msg: format!("expected type id {:?}, found {:?}", TypeId::of::<T>(), any.0.type_id()) }
}
