//! Basic functionality for parsing a PDF file.
pub mod lexer;
pub mod parse_object;
pub mod parse_xref;

use err::*;
use self::lexer::{Lexer, StringLexer};
use primitive::{Primitive, Dictionary, Stream};
use object::{ObjNr, GenNr, PlainRef, Resolve};
use enc::decode;
use types::StreamFilter;
use object::{FromPrimitive, NO_RESOLVE};

/// Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is insufficient.
pub fn parse(data: &[u8]) -> Result<Primitive> {
    parse_with_lexer(&mut Lexer::new(data))
}

/// Recursive. Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is not sufficient.
pub fn parse_with_lexer(lexer: &mut Lexer) -> Result<Primitive> {
    let first_lexeme = lexer.next()?;

    let obj = if first_lexeme.equals(b"<<") {
        let mut dict = Dictionary::default();
        loop {
            // Expect a Name (and Object) or the '>>' delimiter
            let delimiter = lexer.next()?;
            if delimiter.equals(b"/") {
                let key = lexer.next()?.to_string();
                let obj = parse_with_lexer(lexer)?;
                dict.insert(key, obj);
            } else if delimiter.equals(b">>") {
                break;
            } else {
                bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.to_string(), expected: "/ or >>"});
            }
        }
        // It might just be the dictionary in front of a stream.
        if lexer.peek()?.equals(b"stream") {
            lexer.next()?;

            let length = match dict.get("Length") {
                Some(&Primitive::Integer (n)) => n,
                Some(&Primitive::Reference (reference)) => bail!("parse()/parse_with_lexer(): Lenght is found to be an indirect reference."),
                _ => bail!("Length non existent or wrong type."),
            };

            
            let stream_substr = lexer.offset_pos(length as usize);

            // Uncompress/decode if there is a filter
            let content = match dict.get("Filter") {
                Some(filter) => {
                    match *filter {
                        // TODO a lot of clones here
                        Primitive::Name (ref name) => decode(stream_substr.as_slice(), StreamFilter::from_primitive(filter.clone(), NO_RESOLVE)?)?,
                        Primitive::Array (ref filters) => {
                            let mut data = decode(stream_substr.as_slice(), StreamFilter::from_primitive(filters[0].clone(), NO_RESOLVE)?)?;
                            for filter in filters.iter().skip(1) {
                                data = decode(&data, StreamFilter::from_primitive(filter.clone(), NO_RESOLVE)?)?;
                            }
                            data
                        }
                        _ => bail!(ErrorKind::WrongObjectType {expected: "Name or Array", found: filter.get_debug_name()})
                    }
                }
                None => stream_substr.to_vec()
            };
            // Finish
            lexer.next_expect("endstream")?;

            Primitive::Stream(Stream {
                info: dict,
                data: content,
            })
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
        let s = lexer.next()?.to_string();
        Primitive::Name(s)
    } else if first_lexeme.equals(b"[") {
        let mut array = Vec::new();
        // Array
        loop {
            let element = parse_with_lexer(lexer)?;
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
        Primitive::String (hex_str)
    } else {
        bail!("Can't recognize type. Pos: {}\n\tFirst lexeme: {}\n\tRest:\n{}\n\n\tEnd rest\n",
              lexer.get_pos(),
              first_lexeme.to_string(),
              lexer.read_n(50).to_string());
    };

    // trace!("Read object"; "Obj" => format!("{}", obj));

    Ok(obj)
}


pub fn parse_stream(data: &[u8], resolve: &Resolve) -> Result<Stream> {
    parse_stream_with_lexer(&mut Lexer::new(data), resolve)
}


fn parse_stream_with_lexer(lexer: &mut Lexer, r: &Resolve) -> Result<Stream> {
    let first_lexeme = lexer.next()?;

    let obj = if first_lexeme.equals(b"<<") {
        let mut dict = Dictionary::default();
        loop {
            // Expect a Name (and Object) or the '>>' delimiter
            let delimiter = lexer.next()?;
            if delimiter.equals(b"/") {
                let key = lexer.next()?.to_string();
                let obj = parse_with_lexer(lexer)?;
                dict.insert(key, obj);
            } else if delimiter.equals(b">>") {
                break;
            } else {
                bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.to_string(), expected: "/ or >>"});
            }
        }
        // It might just be the dictionary in front of a stream.
        if lexer.peek()?.equals(b"stream") {
            lexer.next()?;

            // Get length - look up in `resolve_fn` if necessary
            let length = match dict.get("Length") {
                Some(&Primitive::Reference (reference)) => match r.resolve(reference)? {
                    Primitive::Integer (n) => n,
                    _ => bail!("Wrong type for stream's /Length."),
                },
                Some(&Primitive::Integer (n)) => n,
                _ => bail!("Length non existent or wrong type."),
            };

            
            let stream_substr = lexer.offset_pos(length as usize);
            // Uncompress/decode if there is a filter
            let content = match dict.get("Filter") {
                Some(filter) => {
                    match *filter {
                        // TODO a lot of clones here
                        Primitive::Name (ref name) => decode(stream_substr.as_slice(), StreamFilter::from_primitive(filter.clone(), NO_RESOLVE)?)?,
                        Primitive::Array (ref filters) => {
                            let mut data = decode(stream_substr.as_slice(), StreamFilter::from_primitive(filters[0].clone(), NO_RESOLVE)?)?;
                            for filter in filters.iter().skip(1) {
                                data = decode(&data, StreamFilter::from_primitive(filter.clone(), NO_RESOLVE)?)?;
                            }
                            data
                        }
                        _ => bail!(ErrorKind::WrongObjectType {expected: "Name or Array", found: filter.get_debug_name()})
                    }
                }
                None => stream_substr.to_vec()
            };
            // Finish
            lexer.next_expect("endstream")?;

            Stream {
                info: dict,
                data: content,
            }
        } else {
            bail!(ErrorKind::WrongObjectType { expected: "Stream", found: "Dictionary" });
        }
    } else {
        bail!(ErrorKind::WrongObjectType { expected: "Stream", found: "something else" });
    };

    Ok(obj)
}


