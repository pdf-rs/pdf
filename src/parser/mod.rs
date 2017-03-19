//! Basic functionality for parsing a PDF file.
pub mod lexer;
mod reader;
//mod writer;
mod parse_object;
mod parse_xref;

pub use self::reader::*;
//pub use self::writer::*;

use err::*;
use self::lexer::{Lexer, StringLexer};
use primitive::{Primitive, Dictionary};
use object::{ObjNr, GenNr, PlainRef};
use stream::Stream;

pub fn parse(data: &[u8]) -> Result<Primitive> {
    parse_internal(&mut Lexer::new(data))
}

/// Recursive.
fn parse_internal(lexer: &mut Lexer) -> Result<Primitive> {
    let first_lexeme = lexer.next()?;

    let obj = if first_lexeme.equals(b"<<") {
        let mut dict = Dictionary::default();
        loop {
            // Expect a Name (and Object) or the '>>' delimiter
            let delimiter = lexer.next()?;
            if delimiter.equals(b"/") {
                let key = lexer.next()?.as_string();
                let obj = parse_internal(lexer)?;
                dict[&key] = obj;
            } else if delimiter.equals(b">>") {
                break;
            } else {
                bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.as_string(), expected: "/ or >>"});
            }
        }
        // It might just be the dictionary in front of a stream.
        if lexer.peek()?.equals(b"stream") {
            bail!("parse() can't parse Stream. Use parse_stream() for that.");
        } else {
            Primitive::Dictionary (dict)
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
                Primitive::Reference (PlainRef {
                    id: first_lexeme.to::<ObjNr>()?,
                    gen: second_lexeme.to::<GenNr>()?,
                })
            } else {
                // We are probably in an array of numbers - it's not a reference anyway
                lexer.set_pos(pos_bk as usize); // (roll back the lexer first)
                Primitive::Integer(first_lexeme.to::<i32>()?)
            }
        } else {
            // It is but a number
            lexer.set_pos(pos_bk as usize); // (roll back the lexer first)
            Primitive::Integer(first_lexeme.to::<i32>()?)
        }
    } else if first_lexeme.is_real_number() {
        // Real Number
        Primitive::Number (first_lexeme.to::<f32>()?)
    } else if first_lexeme.equals(b"/") {
        // Name
        let s = lexer.next()?.as_string();
        Primitive::Name(s)
    } else if first_lexeme.equals(b"[") {
        let mut array = Vec::new();
        // Array
        loop {
            let element = parse_internal(lexer)?;
            array.push(element.clone());

            // Exit if closing delimiter
            if lexer.peek()?.equals(b"]") {
                break;
            }
        }
        lexer.next()?; // Move beyond closing delimiter

        Primitive::Array (array)
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
        lexer.offset_pos(bytes_traversed as usize);

        Primitive::String (string)
    } else if first_lexeme.equals(b"<") {
        let hex_str = lexer.next()?.to_vec();
        lexer.next_expect(">")?;
        Primitive::String (hex_str) // TODO no HexString - problem?
    } else {
        bail!("Can't recognize type. Pos: {}\n\tFirst lexeme: {}\n\tRest:\n{}\n\n\tEnd rest\n",
              lexer.get_pos(),
              first_lexeme.as_string(),
              lexer.read_n(50).as_string());
    };

    // trace!("Read object"; "Obj" => format!("{}", obj));

    Ok(obj)
}


pub fn parse_stream(data: &[u8], resolve_fn: Option<&Fn(PlainRef) -> Result<Primitive>>) -> Result<Primitive> {
    parse_stream_internal(&mut Lexer::new(data), resolve_fn)
}


fn parse_stream_internal(lexer: &mut Lexer, resolve_fn: Option<&Fn(PlainRef) -> Result<Primitive>>) -> Result<Stream> {
    let first_lexeme = lexer.next()?;

    let obj = if first_lexeme.equals(b"<<") {
        let mut dict = Dictionary::default();
        loop {
            // Expect a Name (and Object) or the '>>' delimiter
            let delimiter = lexer.next()?;
            if delimiter.equals(b"/") {
                let key = lexer.next()?.as_string();
                let obj = parse_internal(lexer)?;
                dict[&key] = obj;
            } else if delimiter.equals(b">>") {
                break;
            } else {
                bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.as_string(), expected: "/ or >>"});
            }
        }
        // It might just be the dictionary in front of a stream.
        if lexer.peek()?.equals(b"stream") {
            lexer.next()?;

            // Get length - look up in `resolve_fn` if necessary
            let length = match dict.get("Length")? {
                &Primitive::References (reference) =>
                    match resolve_fn {
                        Some(resolve_fn) =>
                            match resolve_fn(references)? {
                                Primitive::Integer (n) => n,
                                _ => bail!("Wrong type for stream's /Length."),
                            },
                        None => bail!("Reading stream: Can't follow reference without `resolve_fn` function."),
                    },
                &Primitive::Integer (n) => n,
            };

            
            let offset = lexer.get_pos();
            // Skip the stream
            lexer.set_pos(offset + length);
            // Finish
            lexer.next_expect("endstream")?;

            Primitive::Stream (Stream {
                dictionary: dict,
                offset: offset,
                length: length
            })
        } else {
            bail!(ErrorKind::WrongObjectType { expected: "Stream", found: "Dictionary" });
        }
    } else {
        bail!(ErrorKind::WrongObjectType { expected: "Stream", found: "something else" });
    }
}
