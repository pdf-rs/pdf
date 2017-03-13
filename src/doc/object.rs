use doc::Document;
use file;
use err::*;
use std::fmt;
// use std::fmt::{Formatter, Debug};

// Want to wrap file::Primitive together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.


pub trait Object {
    fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()>;
    fn 
}
impl<'a, T: Object + 'a> Object for &'a T {
    fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()> {
        (*self).serialize(out)
    }
}

/// Wraps `file::Primitive`.
pub struct Object<'a> {
    obj: &'a file::Primitive,
    doc: &'a Document,
}


impl<'a> Object<'a> {
    // TODO should only be used by Document
    pub fn new(obj: &'a file::Primitive, doc: &'a Document) -> Object<'a> {
        Object {
            obj: obj,
            doc: doc,
        }
    }
    /// Returns the wrapped Object
    pub fn inner(&self) -> &file::Primitive {
        self.obj
    }
    /// Try to convert to Integer type. Recursively dereference references in the attempt.
    pub fn as_integer(&self) -> Result<i32> {
        match *self.obj {
            file::Primitive::Integer (n) => Ok(n),
            file::Primitive::Reference (id) => self.doc.get_object(id)?.as_integer(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Integer or Reference", found: self.obj.type_str()}.into()),
        }
    }

    pub fn as_number(&self) -> Result<f32> {
        match *self.obj {
            file::Primitive::Number (n) => Ok(n),
            file::Primitive::Reference (id) => self.doc.get_object(id)?.as_number(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Number or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Dictionary type. Recursively dereference references in the attempt.
    pub fn as_dictionary(&self) -> Result<Dictionary<'a>> {
        match *self.obj {
            file::Primitive::Dictionary (ref dict) => Ok(Dictionary {dict: dict, doc: self.doc}),
            file::Primitive::Reference (id) => self.doc.get_object(id)?.as_dictionary(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Dictionary or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Stream type. Recursively dereference references in the attempt.
    pub fn as_stream(&self) -> Result<Stream<'a>> {
        match *self.obj {
            file::Primitive::Stream (ref stream) => {
                Ok(Stream {
                    dict: Dictionary {dict: &stream.dictionary, doc: self.doc},
                    content: &stream.content,
                    doc: self.doc
                })
            }
            file::Primitive::Reference (id) => self.doc.get_object(id)?.as_stream(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Stream or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Array type. Recursively dereference references in the attempt.
    pub fn as_array(&self) -> Result<Array<'a>> {
        match *self.obj {
            file::Primitive::Array (ref array) => Ok(Array {array: array, doc: self.doc}),
            file::Primitive::Reference (id) => self.doc.get_object(id)?.as_array(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Array or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Name type. Recursively dereference references in the attempt.
    pub fn as_name(&self) -> Result<String> {
        match *self.obj {
            file::Primitive::Name(ref s) => Ok(s.clone()),
            file::Primitive::Reference(id) => self.doc.get_object(id)?.as_name(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Name or Reference", found: self.obj.type_str()}.into()),
        }
    }
}


/// Wraps `file::Stream`.
#[derive(Clone)]
pub struct Stream<'a> {
    pub dict: Dictionary<'a>,
    pub content: &'a Vec<u8>,
    doc: &'a Document,
}

/* TODO
impl<'a> Stream<'a> {
    ///
    pub fn parse_content()
}
*/

/// Wraps `file::Array`.
#[derive(Clone)]
pub struct Array<'a> {
    array: &'a Vec<file::Primitive>,
    doc: &'a Document,
}

impl<'a> Array<'a> {
    pub fn len(&self) -> usize {
        self.array.len()
    }
    pub fn is_empty(&self) -> bool {
        self.array.len() == 0
    }
    pub fn get(&self, index: usize) -> Object<'a> {
        Object {
            obj: &self.array[index],
            doc: self.doc,
        }
    }

}

impl<'a: 'b, 'b> IntoIterator for &'b Array<'a> {
    type Item = Object<'a>;
    type IntoIter = ArrayIter<'a, 'b>;
    fn into_iter(self) -> ArrayIter<'a, 'b> {
        ArrayIter {
            array: self,
            index: 0,
        }
    }
}

pub struct ArrayIter<'a: 'b, 'b> {
    array: &'b Array<'a>,
    index: usize,
}
impl<'a: 'b, 'b> Iterator for ArrayIter<'a, 'b> {
    type Item = Object<'a>;
    fn next(&mut self) -> Option<Object<'a>> {
        if self.index < self.array.len() {
            self.index += 1;
            Some(self.array.get(self.index-1))
        } else {
            None
        }
    }
}

/*
impl<'a> Index<usize> for Array<'a> {
    type Output = Object<'a>;
    fn index(&self, index: usize) -> &Object<'a> {
        &Object {
            obj: &self.array[0],
            doc: &self.doc,
        }
        // panic!();
    }
}
*/


/* fmt::Debug for wrappers: */

impl<'a> fmt::Debug for Object<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.obj)
    }
}
impl<'a> fmt::Debug for Dictionary<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.dict)
    }
}
impl<'a> fmt::Debug for Array<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.array)
    }
}
impl<'a> fmt::Debug for Stream<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Dict: {:?}, Content: {:?}", self.dict, self.content)
    }
}
