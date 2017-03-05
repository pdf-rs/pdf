use doc::Document;
use file;
use err::*;
use std::ops::Index;

// Want to wrap file::Object together with Document, so that we may do dereferencing.
// e.g.
// my_obj.as_integer() will dereference if needed.


/// Wrapper for `file::Object`.
pub struct Object<'a> {
    obj: &'a file::Object,
    doc: &'a Document,
}


impl<'a> Object<'a> {
    // TODO should only be used by Document
    pub fn new(obj: &'a file::Object, doc: &'a Document) -> Object<'a> {
        Object {
            obj: obj,
            doc: doc,
        }
    }
    /// Returns the wrapped Object
    pub fn inner(&self) -> &file::Object {
        self.obj
    }
    /// Try to convert to Integer type. Recursively dereference references in the attempt.
    pub fn as_integer(&self) -> Result<i32> {
        match self.obj {
            &file::Object::Integer (n) => Ok(n),
            &file::Object::Reference (id) => self.doc.get_object(id)?.as_integer(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Integer or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Dictionary type. Recursively dereference references in the attempt.
    pub fn as_dictionary(&self) -> Result<Dictionary<'a>> {
        match self.obj {
            &file::Object::Dictionary (ref dict) => Ok(Dictionary {dict: dict, doc: &self.doc}),
            &file::Object::Reference (id) => self.doc.get_object(id)?.as_dictionary(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Dictionary or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Stream type. Recursively dereference references in the attempt.
    pub fn as_stream(&self) -> Result<Stream<'a>> {
        match self.obj {
            &file::Object::Stream (ref stream) => {
                Ok(Stream {
                    dict: Dictionary {dict: &stream.dictionary, doc: &self.doc},
                    content: &stream.content,
                    doc: &self.doc
                })
            }
            &file::Object::Reference (id) => self.doc.get_object(id)?.as_stream(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Stream or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Array type. Recursively dereference references in the attempt.
    pub fn as_array(&self) -> Result<Array<'a>> {
        match self.obj {
            &file::Object::Array (ref array) => Ok(Array {array: array, doc: &self.doc}),
            &file::Object::Reference (id) => self.doc.get_object(id)?.as_array(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Array or Reference", found: self.obj.type_str()}.into()),
        }
    }

    /// Try to convert to Name type. Recursively dereference references in the attempt.
    pub fn as_name(&self) -> Result<String> {
        match self.obj {
            &file::Object::Name(ref s) => Ok(s.clone()),
            &file::Object::Reference(id) => self.doc.get_object(id)?.as_name(),
            _ => Err (ErrorKind::WrongObjectType {expected: "Name or Reference", found: self.obj.type_str()}.into()),
        }
    }
}


/// Wraps `file::Dictionary`.
#[derive(Copy, Clone)]
pub struct Dictionary<'a> {
    dict: &'a file::Dictionary,
    doc: &'a Document,
}

impl<'a> Dictionary<'a> {
    pub fn get<K>(&self, key: K) -> Result<Object<'a>>
        where K: Into<String>
    {
        let key = key.into();
        Ok(Object {
            obj: self.dict.get(key)?,
            doc: self.doc,
        })
    }
}

/// Wraps `file::Stream`.
// 
#[derive(Clone)]
pub struct Stream<'a> {
    pub dict: Dictionary<'a>,
    pub content: &'a Vec<u8>,
    doc: &'a Document,
}

/// Wraps `file::Array`.
#[derive(Clone)]
pub struct Array<'a> {
    array: &'a Vec<file::Object>,
    doc: &'a Document,
}

impl<'a> Array<'a> {
    pub fn len(&self) -> usize {
        self.array.len()
    }
    pub fn get(&self, index: usize) -> Object<'a> {
        Object {
            obj: &self.array[index],
            doc: &self.doc,
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
