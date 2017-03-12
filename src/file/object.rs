use err::*;
use file::types::*;


use std;
use std::io::Write;
use std::vec::Vec;
use std::str::from_utf8;
use std::fmt::{Display, Formatter};
use std::collections::HashMap;


pub trait Object {
    fn serialize<W: Write>(&self, out: &mut W) -> Result<()>;
}

/* Objects */
pub struct IndirectObject {
    pub id: ObjectId,
    pub object: AnyObject,
}

#[derive(Clone, Debug)]
pub enum AnyObject {
    Null,
    Integer (i32),
    Number (f32),
    Boolean (bool),
    String (Vec<u8>),
    /// Each byte is 0-15
    HexString (Vec<u8>),
    Stream (Stream),
    Dictionary (Dictionary),
    Array (Vec<AnyObject>),
    Reference (ObjectId),
    Name (String),
}

/// PDF stream object.
#[derive(Clone, Debug)]
pub struct Stream {
    pub dictionary: Dictionary,
    pub content: Vec<u8>,
}

/// Used to identify an object; corresponds to a PDF indirect reference.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId {
    pub obj_nr: u32,
    pub gen_nr: u16,
}




/// PDF dictionary object, maps from `String` to `file::Object`.
#[derive(Clone, Debug, Default)]
pub struct Dictionary (pub HashMap<String, AnyObject>);

impl Dictionary {
    pub fn get<K>(&self, key: K) -> Result<&AnyObject>
        where K: Into<String>
    {
        let key = key.into();
        self.0.get(&key).ok_or_else(|| ErrorKind::NotFound {word: key}.into())
    }
    pub fn set<K, V>(&mut self, key: K, value: V)
		where K: Into<String>,
		      V: Into<AnyObject>
	{
		let _ = self.0.insert(key.into(), value.into());
	}

    /// Mostly used for debugging. If type is not specified, it will return Ok(()).
    pub fn expect_type<K>(&self, type_name: K) -> Result<()>
        where K: Into<String>
    {
        let type_name = type_name.into();
        match self.get("Type") {
            Err(_) => Ok(()),
            Ok(&AnyObject::Name (ref name)) => {
                if *name == *type_name {
                    Ok(())
                } else {
                    bail!("Expected type {}, found type {}.", type_name, name)
                }
            }
            _ => bail!("???"),
        }
    }
}

impl AnyObject {
    pub fn as_integer(&self) -> Result<i32> {
        match *self {
            AnyObject::Integer (n) => Ok(n),
            _ => {
                // Err (ErrorKind::WrongObjectType.into()).chain_err(|| ErrorKind::ExpectedType {expected: "Reference"})
                Err (ErrorKind::WrongObjectType {expected: "Integer", found: self.type_str()}.into())
            }
        }
    }
    pub fn as_reference(&self) -> Result<ObjectId> {
        match *self {
            AnyObject::Reference (id) => Ok(id),
            _ => {
                Err (ErrorKind::WrongObjectType {expected: "Reference", found: self.type_str()}.into())
            }
        }
    }
    pub fn as_array(&self) -> Result<&Vec<AnyObject>> {
        match *self {
            AnyObject::Array (ref v) => Ok(v),
            _ => Err (ErrorKind::WrongObjectType {expected: "Array", found: self.type_str()}.into())
        }
    }
    pub fn as_integer_array(&self) -> Result<Vec<i32>> {
        self.as_array()?.iter()
            .map(|x| Ok(x.as_integer()?)).collect::<Result<Vec<_>>>()
    }

    pub fn as_dictionary(&self) -> Result<&Dictionary> {
        match *self {
            AnyObject::Dictionary (ref dict) => Ok(dict),
            _ => Err (ErrorKind::WrongObjectType {expected: "Dictionary", found: self.type_str()}.into())
        }
    }

    pub fn as_stream(&self) -> Result<&Stream> {
        match *self {
            AnyObject::Stream (ref s) => Ok(s),
            _ => Err (ErrorKind::WrongObjectType {expected: "Stream", found: self.type_str()}.into()),
        }
    }

    pub fn into_array(self) -> Result<Vec<AnyObject>> {
        match self {
            AnyObject::Array (v) => Ok(v),
            _ => Err (ErrorKind::WrongObjectType {expected: "Array", found: self.type_str()}.into())
        }
    }
    pub fn into_integer_array(self) -> Result<Vec<i32>> {
        self.as_array()?.iter()
            .map(|x| Ok(x.as_integer()?)).collect::<Result<Vec<_>>>()
    }

    pub fn into_dictionary(self) -> Result<Dictionary> {
        match self {
            AnyObject::Dictionary (dict) => Ok(dict),
            _ => Err (ErrorKind::WrongObjectType {expected: "Dictionary", found: self.type_str()}.into())
        }
    }

    pub fn into_stream(self) -> Result<Stream> {
        match self {
            AnyObject::Stream (s) => Ok(s),
            _ => Err (ErrorKind::WrongObjectType {expected: "Stream", found: self.type_str()}.into()),
        }
    }

    pub fn type_str(&self) -> &'static str {
        match *self {
            AnyObject::Integer (_) => "Integer",
            AnyObject::Number (_) => "Number",
            AnyObject::Boolean (_) => "Boolean",
            AnyObject::String (_) => "String",
            AnyObject::HexString (_) => "HexString",
            AnyObject::Stream (_) => "Stream",
            AnyObject::Dictionary (_) => "Dictionary",
            AnyObject::Array (_) => "Array",
            AnyObject::Reference (_) => "Reference",
            AnyObject::Name (_) => "Name",
            AnyObject::Null => "Null",
        }
    }

}


impl Display for ObjectId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "obj_id({} {})", self.obj_nr, self.gen_nr)
    }
}

impl Display for AnyObject {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match *self {
            AnyObject::Integer(n) => write!(f, "{}", n),
            AnyObject::Number(n) => write!(f, "{}", n),
            AnyObject::Boolean(b) => write!(f, "{}", if b {"true"} else {"false"}),
            AnyObject::String (ref s) => {
                let decoded = from_utf8(s);
                match decoded {
                    Ok(decoded) => write!(f, "({})", decoded),
                    Err(_) => {
                        // Write out bytes as numbers.
                        write!(f, "encoded(")?;
                        for c in s {
                            write!(f, "{},", c)?;
                        }
                        write!(f, ")")
                    }
                }
            },
            AnyObject::HexString (ref s) => {
                for c in s {
                    write!(f, "{},", c)?;
                }
                Ok(())
            }
            AnyObject::Stream (ref stream) => {
                write!(f, "{}", stream)
            }
            AnyObject::Dictionary(Dictionary(ref d)) => {
                write!(f, "<< ")?;
                for e in d {
                    write!(f, "/{} {}", e.0, e.1)?;
                }
                write!(f, ">>\n")
            },
            AnyObject::Array(ref a) => {
                write!(f, "[")?;
                for e in a {
                    write!(f, "{} ", e)?;
                }
                write!(f, "]")
            },
            AnyObject::Reference (ObjectId {obj_nr, gen_nr}) => {
                write!(f, "{} {} R", obj_nr, gen_nr)
            },
            AnyObject::Name (ref name) => write!(f, "/{}", name),
            AnyObject::Null => write!(f, "Null"),
        }
    }
}

impl Display for Stream {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let decoded = from_utf8(&self.content);
        match decoded {
            Ok(decoded) => write!(f, "stream\n{}\nendstream\n", decoded),
            Err(_) => {
                // Write out bytes as numbers.
                write!(f, "stream\n{:?}\nendstream\n", self.content)
            }
        }
    }
}

