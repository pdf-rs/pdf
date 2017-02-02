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


/* Objects */
pub struct IndirectObject {
    pub obj_nr: i32,
    pub gen_nr: i32,
    pub object: Object,
}

#[derive(Clone, Debug)]
pub enum Object {
    Integer (i32),
    RealNumber (f32),
    Boolean (bool),
    String (Vec<u8>),
    HexString (Vec<u8>), // each byte is 0-15
    Stream {dictionary: Vec<(String, Object)>, content: Vec<u8>},
    Dictionary (Vec<(String, Object)>),
    Array (Vec<Object>),
    Reference {obj_nr: i32, gen_nr: i32},
    Name (String),
    Null,
}

impl IndirectObject {
    pub fn parse_from(lexer: &mut Lexer) -> Result<IndirectObject> {
        trace!("-> read_indirect_object_from");
        let obj_nr = lexer.next()?.to::<i32>()?;
        let gen_nr = lexer.next()?.to::<i32>()?;
        lexer.next_expect("obj")?;

        let obj = Object::parse_from(lexer)?;

        lexer.next_expect("endobj")?;

        trace!("- read_indirect_object_from");
        Ok(IndirectObject {
            obj_nr: obj_nr,
            gen_nr: gen_nr,
            object: obj,
        })
    }
}

impl Object {
    /// `self` must be an `Object::Dictionary` or an `Object::Stream`.
    pub fn dict_get<'a>(&'a self, key: &'static str) -> Result<&'a Object> {
        match self {
            &Object::Dictionary (ref dictionary) => {
                for &(ref name, ref object) in dictionary {
                    if key == *name {
                        return Ok(object);
                    }
                }
                Err (ErrorKind::NotFound {word: key.to_string()}.into())
            },
            &Object::Stream {ref dictionary, content: _} => {
                for &(ref name, ref object) in dictionary {
                    if key == *name {
                        return Ok(object);
                    }
                }
                Err (ErrorKind::NotFound {word: key.to_string()}.into())
            }
            _ => {
                Err (ErrorKind::WrongObjectType.into())
            }
        }
    }
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

    // TODO: Notice how sometimes we peek(), and in one branch we do next() in order to move
    // forward. Consider having a back() instead of next()?
    pub fn parse_from(lexer: &mut Lexer) -> Result<Object> {
        let first_lexeme = lexer.next()?;

        let obj = if first_lexeme.equals(b"<<") {
            let mut dictionary = Vec::new();
            loop {
                // Expect a Name (and Object) or the '>>' delimiter
                let delimiter = lexer.next()?;
                if delimiter.equals(b"/") {
                    let key = lexer.next()?.as_string();
                    trace!("Dict add"; "Key" => key);
                    let obj = Object::parse_from(lexer)?;
                    trace!("Dict add"; "Obj" => obj.to_string());
                    dictionary.push( (key, obj) );
                } else if delimiter.equals(b">>") {
                    break;
                } else {
                    println!("Dicionary in progress: {:?}", dictionary);
                    bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.as_string(), expected: "/ or >>"});
                }
            }
            // It might just be the dictionary in front of a stream.
            let dict = Object::Dictionary(dictionary.clone());
            if lexer.peek()?.equals(b"stream") {
                lexer.next()?;

                // Get length
                let length_obj = dict.dict_get("Length".into())?;

                let length = length_obj.unwrap_integer()?;
                // Read the stream
                let content = lexer.seek(SeekFrom::Current(length as i64));
                debug!("Stream"; "contents" => content.as_string());
                // Finish
                lexer.next_expect("endstream")?;

                Object::Stream {
                    dictionary: dictionary,
                    content: content.to_vec(),
                }
            } else {
                dict
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
                    Object::Reference {
                        obj_nr: first_lexeme.to::<i32>()?,
                        gen_nr: second_lexeme.to::<i32>()?,
                    }
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
            &Object::Stream{dictionary: _, ref content} => {
                let decoded = from_utf8(content);
                match decoded {
                    Ok(decoded) => write!(f, "stream\n{}\nendstream\n", decoded),
                    Err(_) => {
                        // Write out bytes as numbers.
                        write!(f, "stream\n{:?}\nendstream\n", content)
                    }
                }
            }
            &Object::Dictionary(ref d) => {
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
            &Object::Reference{obj_nr, gen_nr} => {
                write!(f, "{} {} R", obj_nr, gen_nr)
            },
            &Object::Name (ref name) => write!(f, "/{}", name),
            &Object::Null => write!(f, "Null"),
        }
    }
}

