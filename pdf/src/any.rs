use std::any::TypeId;
use std::rc::Rc;
use crate::object::Object;

pub trait AnyObject {
    fn serialize(&self, out: &mut Vec<u8>);
    fn type_name(&self) -> &'static str;
    fn type_id(&self) -> TypeId;
}
impl<T> AnyObject for T
    where T: Object + 'static
{
    fn serialize(&self, out: &mut Vec<u8>) {
        Object::serialize(self, out).expect("write error on Vec<u8> ?!?")
    }
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
    pub fn downcast<T>(self) -> Option<Rc<T>> 
        where T: AnyObject + 'static
    {
        if TypeId::of::<T>() == self.0.type_id() {
            unsafe {
                let raw: *const dyn AnyObject = Rc::into_raw(self.0);
                Some(Rc::from_raw(raw as *const T))
            }
        } else {
            None
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
