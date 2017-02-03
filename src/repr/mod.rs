//! Runtime representation of a PDF file.

mod xref;

pub use self::xref::*;

use std::vec::Vec;
use err::*;
use reader::lexer::Lexer;
use reader::lexer::StringLexer;


use std;
use std::str::from_utf8;
use std::fmt::{Display, Formatter};
use std::io::SeekFrom;
use std::collections::HashMap;


/* Objects */
pub struct IndirectObject {
    pub id: ObjectId,
    pub object: Object,
}

#[derive(Clone, Debug)]
pub enum Object {
    Integer (i32),
    RealNumber (f32),
    Boolean (bool),
    String (Vec<u8>),
    HexString (Vec<u8>), // each byte is 0-15
    Stream (Stream),
    Dictionary (Dictionary),
    Array (Vec<Object>),
    Reference (ObjectId),
    Name (String),
    Null,
}

#[derive(Clone, Debug)]
pub struct Dictionary (HashMap<String, Object>);

#[derive(Clone, Debug)]
pub struct Stream {
    pub dictionary: Dictionary,
    pub content: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ObjectId {
    pub obj_nr: u32,
    pub gen_nr: u16,
}



impl IndirectObject {
    pub fn parse_from(lexer: &mut Lexer) -> Result<IndirectObject> {
        trace!("-> read_indirect_object_from");
        let obj_nr = lexer.next()?.to::<u32>()?;
        let gen_nr = lexer.next()?.to::<u16>()?;
        lexer.next_expect("obj")?;

        let obj = Object::parse_from(lexer)?;

        lexer.next_expect("endobj")?;

        trace!("- read_indirect_object_from");
        Ok(IndirectObject {
            id: ObjectId {obj_nr: obj_nr, gen_nr: gen_nr},
            object: obj,
        })
    }
}

impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary (HashMap::new())
    }
    pub fn get<'a, K>(&'a self, key: K) -> Result<&'a Object>
        where K: Into<String>
    {
        let key = key.into();
        self.0.get(&key).ok_or(ErrorKind::NotFound {word: key}.into())
    }
    pub fn set<K, V>(&mut self, key: K, value: V)
		where K: Into<String>,
		      V: Into<Object>
	{
		let _ = self.0.insert(key.into(), value.into());
	}
}

impl Object {
    pub fn unwrap_integer(&self) -> Result<i32> {
        match self {
            &Object::Integer (n) => Ok(n),
            _ => {
                // Err (ErrorKind::WrongObjectType.into()).chain_err(|| ErrorKind::ExpectedType {expected: "Reference"})
                Err (ErrorKind::WrongObjectType.into())
            }
        }
    }
    pub fn unwrap_array(&self) -> Result<&Vec<Object>> {
        match self {
            &Object::Array (ref v) => Ok(v),
            _ => Err (ErrorKind::WrongObjectType.into())
        }
    }
    pub fn unwrap_integer_array(&self) -> Result<Vec<i32>> {
        self.unwrap_array()?.iter()
            .map(|x| Ok(x.unwrap_integer()?)).collect::<Result<Vec<_>>>()
    }

    pub fn unwrap_dictionary(self) -> Result<Dictionary> {
        match self {
            Object::Dictionary (dict) => Ok(dict),
            _ => Err (ErrorKind::WrongObjectType.into())
        }
    }

    pub fn unwrap_stream(self) -> Result<Stream> {
        match self {
            Object::Stream (s) => Ok(s),
            _ => Err (ErrorKind::WrongObjectType.into()),
        }
    }

    pub fn parse_from(lexer: &mut Lexer) -> Result<Object> {
        let first_lexeme = lexer.next()?;

        let obj = if first_lexeme.equals(b"<<") {

            let mut dict = Dictionary::new();
            loop {
                // Expect a Name (and Object) or the '>>' delimiter
                let delimiter = lexer.next()?;
                if delimiter.equals(b"/") {
                    let key = lexer.next()?.as_string();
                    let obj = Object::parse_from(lexer)?;
                    dict.set(key, obj);
                } else if delimiter.equals(b">>") {
                    break;
                } else {
                    bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.as_string(), expected: "/ or >>"});
                }
            }
            // It might just be the dictionary in front of a stream.
            if lexer.peek()?.equals(b"stream") {
                lexer.next()?;

                // Get length
                let length = { dict.get("Length")?.unwrap_integer()? };
                // Read the stream
                let content = lexer.seek(SeekFrom::Current(length as i64));
                debug!("Stream"; "contents" => content.as_string());
                // Finish
                lexer.next_expect("endstream")?;

                Object::Stream (Stream {
                    dictionary: dict,
                    content: content.to_vec(),
                })
            } else {
                Object::Dictionary (dict)
            }
        } else if first_lexeme.is_integer() {
            // May be Integer or Reference

            // First backup position
            let pos_bk = lexer.get_pos();
            
            let second_lexeme = lexer.next()?;
            if second_lexeme.is_integer() {
                let third_lexeme = lexer.next()?;
                if third_lexeme.equals(b"R") {
                    // It is indeed a reference to an indirect object
                    Object::Reference (ObjectId {
                        obj_nr: first_lexeme.to::<u32>()?,
                        gen_nr: second_lexeme.to::<u16>()?,
                    })
                } else {
                    // We are probably in an array of numbers - it's not a reference anyway
                    lexer.seek(SeekFrom::Start(pos_bk as u64)); // (roll back the lexer first)
                    Object::Integer(first_lexeme.to::<i32>()?)
                }
            } else {
                // It is but a number
                lexer.seek(SeekFrom::Start(pos_bk as u64)); // (roll back the lexer first)
                Object::Integer(first_lexeme.to::<i32>()?)
            }
        } else if first_lexeme.equals(b"/") {
            // Name
            let s = lexer.next()?.as_string();
            Object::Name(s)
        } else if first_lexeme.equals(b"[") {
            let mut array = Vec::new();
            // Array
            loop {
                let element = Object::parse_from(lexer)?;
                array.push(element);

                // Exit if closing delimiter
                if lexer.peek()?.equals(b"]") {
                    break;
                }
            }
            lexer.next()?; // Move beyond closing delimiter

            Object::Array (array)
        } else if first_lexeme.equals(b"(") {

            let mut string: Vec<u8> = Vec::new();

            let bytes_traversed = {
                let mut string_lexer = StringLexer::new(lexer.get_remaining_slice());
                for character in string_lexer.iter() {
                    let character = character?;
                    string.push(character);
                }
                string_lexer.get_offset() as i64
            };
            // Advance to end of string
            lexer.seek(SeekFrom::Current (bytes_traversed));

            Object::String (string)
        } else if first_lexeme.equals(b"<") {
            let hex_str = lexer.next()?.to_vec();
            lexer.next_expect(">")?;
            Object::HexString (hex_str)
        } else {
            bail!("Can't recognize type. Pos: {}\n\tFirst lexeme: {}\n\tRest:\n{}\n\n\tEnd rest\n",
                  lexer.get_pos(),
                  first_lexeme.as_string(),
                  lexer.read_n(50).as_string());
        };

        // trace!("Read object"; "Obj" => format!("{}", obj));

        Ok(obj)
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            &Object::Integer(n) => write!(f, "{}", n),
            &Object::RealNumber(n) => write!(f, "{}", n),
            &Object::Boolean(b) => write!(f, "{}", if b {"true"} else {"false"}),
            &Object::String (ref s) => {
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
            &Object::HexString (ref s) => {
                for c in s {
                    write!(f, "{},", c)?;
                }
                Ok(())
            }
            &Object::Stream (Stream {dictionary: _, ref content}) => {
                let decoded = from_utf8(content);
                match decoded {
                    Ok(decoded) => write!(f, "stream\n{}\nendstream\n", decoded),
                    Err(_) => {
                        // Write out bytes as numbers.
                        write!(f, "stream\n{:?}\nendstream\n", content)
                    }
                }
            }
            &Object::Dictionary(Dictionary(ref d)) => {
                write!(f, "<< ")?;
                for e in d {
                    write!(f, "/{} {}", e.0, e.1)?;
                }
                write!(f, ">>\n")
            },
            &Object::Array(ref a) => {
                write!(f, "[")?;
                for e in a {
                    write!(f, "{} ", e)?;
                }
                write!(f, "]")
            },
            &Object::Reference (ObjectId {obj_nr, gen_nr}) => {
                write!(f, "{} {} R", obj_nr, gen_nr)
            },
            &Object::Name (ref name) => write!(f, "/{}", name),
            &Object::Null => write!(f, "Null"),
        }
    }
}

