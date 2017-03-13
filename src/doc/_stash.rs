

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

