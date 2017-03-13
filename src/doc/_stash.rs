
impl Primitive {
    pub fn as_integer(&self) -> Result<i32> {
        match *self {
            Primitive::Integer (n) => Ok(n),
            _ => {
                // Err (ErrorKind::WrongObjectType.into()).chain_err(|| ErrorKind::ExpectedType {expected: "Reference"})
                Err (ErrorKind::WrongObjectType {expected: "Integer", found: self.type_str()}.into())
            }
        }
    }
    pub fn as_reference(&self) -> Result<ObjectId> {
        match *self {
            Primitive::Reference (id) => Ok(id),
            _ => {
                Err (ErrorKind::WrongObjectType {expected: "Reference", found: self.type_str()}.into())
            }
        }
    }
    pub fn as_array(&self) -> Result<&Vec<Primitive>> {
        match *self {
            Primitive::Array (ref v) => Ok(v),
            _ => Err (ErrorKind::WrongObjectType {expected: "Array", found: self.type_str()}.into())
        }
    }
    pub fn as_integer_array(&self) -> Result<Vec<i32>> {
        self.as_array()?.iter()
            .map(|x| Ok(x.as_integer()?)).collect::<Result<Vec<_>>>()
    }

    pub fn as_dictionary(&self) -> Result<&Dictionary> {
        match *self {
            Primitive::Dictionary (ref dict) => Ok(dict),
            _ => Err (ErrorKind::WrongObjectType {expected: "Dictionary", found: self.type_str()}.into())
        }
    }

    pub fn as_stream(&self) -> Result<&Stream> {
        match *self {
            Primitive::Stream (ref s) => Ok(s),
            _ => Err (ErrorKind::WrongObjectType {expected: "Stream", found: self.type_str()}.into()),
        }
    }

    pub fn into_array(self) -> Result<Vec<Primitive>> {
        match self {
            Primitive::Array (v) => Ok(v),
            _ => Err (ErrorKind::WrongObjectType {expected: "Array", found: self.type_str()}.into())
        }
    }
    pub fn into_integer_array(self) -> Result<Vec<i32>> {
        self.as_array()?.iter()
            .map(|x| Ok(x.as_integer()?)).collect::<Result<Vec<_>>>()
    }

    pub fn into_dictionary(self) -> Result<Dictionary> {
        match self {
            Primitive::Dictionary (dict) => Ok(dict),
            _ => Err (ErrorKind::WrongObjectType {expected: "Dictionary", found: self.type_str()}.into())
        }
    }

    pub fn into_stream(self) -> Result<Stream> {
        match self {
            Primitive::Stream (s) => Ok(s),
            _ => Err (ErrorKind::WrongObjectType {expected: "Stream", found: self.type_str()}.into()),
        }
    }

    pub fn type_str(&self) -> &'static str {
        match *self {
            Primitive::Integer (_) => "Integer",
            Primitive::Number (_) => "Number",
            Primitive::Boolean (_) => "Boolean",
            Primitive::String (_) => "String",
            Primitive::HexString (_) => "HexString",
            Primitive::Stream (_) => "Stream",
            Primitive::Dictionary (_) => "Dictionary",
            Primitive::Array (_) => "Array",
            Primitive::Reference (_) => "Reference",
            Primitive::Name (_) => "Name",
            Primitive::Null => "Null",
        }
    }

}


impl Display for ObjectId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "obj_id({} {})", self.obj_nr, self.gen_nr)
    }
}

impl Display for Primitive {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match *self {
            Primitive::Integer(n) => write!(f, "{}", n),
            Primitive::Number(n) => write!(f, "{}", n),
            Primitive::Boolean(b) => write!(f, "{}", if b {"true"} else {"false"}),
            Primitive::String (ref s) => {
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
            Primitive::HexString (ref s) => {
                for c in s {
                    write!(f, "{},", c)?;
                }
                Ok(())
            }
            Primitive::Stream (ref stream) => {
                write!(f, "{}", stream)
            }
            Primitive::Dictionary(ref dict) => {
                write!(f, "<< ")?;
                for (ref key, ref val) in &dict {
                    write!(f, "/{} {}", key, val)?;
                }
                write!(f, ">>\n")
            },
            Primitive::Array(ref a) => {
                write!(f, "[")?;
                for e in a {
                    write!(f, "{} ", e)?;
                }
                write!(f, "]")
            },
            Primitive::Reference (ObjectId {obj_nr, gen_nr}) => {
                write!(f, "{} {} R", obj_nr, gen_nr)
            },
            Primitive::Name (ref name) => write!(f, "/{}", name),
            Primitive::Null => write!(f, "Null"),
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

