use doc::Document;
use file;
use err::*;

// Want to wrap file::Object together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.


/// `Object` wraps a `file::Object` together with a reference to `Document`, in order to be able to
/// dereference objects when needed.
pub struct Object<'a> {
    obj: &'a file::Object,
    doc: &'a Document,
}




impl<'a> Object<'a> {
    pub fn new(obj: &'a file::Object, doc: &'a Document) -> Object<'a> {
        Object {
            obj: obj,
            doc: doc,
        }
    }
    pub fn as_integer(&self) -> Result<i32> {
        match self.obj {
            &file::Object::Integer (n) => Ok(n),
            &file::Object::Reference (id) => self.doc.get_object(id)?.as_integer(),
            _ => Err (ErrorKind::WrongObjectType.into()),
        }
    }
}
