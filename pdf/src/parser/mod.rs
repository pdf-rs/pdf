//! Basic functionality for parsing a PDF file.

mod lexer;
mod parse_object;
mod parse_xref;

pub use self::lexer::*;
pub use self::parse_object::*;
pub use self::parse_xref::*;

use self::lexer::{HexStringLexer, StringLexer};
use crate::crypt::Decoder;
use crate::error::*;
use crate::object::{GenNr, ObjNr, PlainRef, Resolve};
use crate::primitive::{Dictionary, PdfStream, PdfString, Primitive};

const MAX_DEPTH: usize = 20;

pub struct Context<'a> {
    pub decoder: Option<&'a Decoder>,
    pub obj_nr:  u64,
    pub gen_nr:  u16,
}
impl<'a> Context<'a> {
    pub fn decrypt<'buf>(&self, data: &'buf mut [u8]) -> Result<&'buf [u8]> {
        if let Some(decoder) = self.decoder {
            decoder.decrypt(self.obj_nr, self.gen_nr, data)
        } else {
            Ok(data)
        }
    }
}

/// Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is insufficient.
pub fn parse(data: &[u8], r: &impl Resolve) -> Result<Primitive> {
    parse_with_lexer(&mut Lexer::new(data), r)
}

/// Recursive. Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is not sufficient.
pub fn parse_with_lexer(lexer: &mut Lexer, r: &impl Resolve) -> Result<Primitive> {
    parse_with_lexer_ctx(lexer, r, None, MAX_DEPTH)
}

fn parse_dictionary_object(
    lexer: &mut Lexer,
    r: &impl Resolve,
    ctx: Option<&Context>,
    max_depth: usize,
) -> Result<Dictionary> {
    let mut dict = Dictionary::default();
    loop {
        // Expect a Name (and Object) or the '>>' delimiter
        let token = t!(lexer.next());
        if token.starts_with(b"/") {
            let key = token.reslice(1..).to_string();
            let obj = t!(parse_with_lexer_ctx(lexer, r, ctx, max_depth));
            dict.insert(key, obj);
        } else if token.equals(b">>") {
            break;
        } else {
            err!(PdfError::UnexpectedLexeme {
                pos:      lexer.get_pos(),
                lexeme:   token.to_string(),
                expected: "/ or >>",
            });
        }
    }
    Ok(dict)
}

fn parse_stream_object(
    dict: Dictionary,
    lexer: &mut Lexer,
    r: &impl Resolve,
    ctx: Option<&Context>,
) -> Result<PdfStream> {
    t!(lexer.next_stream());

    let length = match dict.get("Length") {
        Some(&Primitive::Integer(n)) => n,
        Some(&Primitive::Reference(reference)) => t!(t!(r.resolve(reference)).as_integer()),
        Some(other) => err!(PdfError::UnexpectedPrimitive {
            expected: "Integer or Reference",
            found:    other.get_debug_name(),
        }),
        None => err!(PdfError::MissingEntry {
            typ:   "<Stream>",
            field: "Length".into(),
        }),
    };

    let stream_substr = lexer.read_n(length as usize);

    // Finish
    t!(lexer.next_expect("endstream"));
    let mut data = stream_substr.to_vec();

    // decrypt it
    if let Some(ctx) = ctx {
        data = t!(ctx.decrypt(&mut data)).to_vec();
    }

    Ok(PdfStream { info: dict, data })
}

/// Recursive. Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is not sufficient.
pub fn parse_with_lexer_ctx(
    lexer: &mut Lexer,
    r: &impl Resolve,
    ctx: Option<&Context>,
    max_depth: usize,
) -> Result<Primitive> {
    let first_lexeme = t!(lexer.next());

    let obj = if first_lexeme.equals(b"<<") {
        if max_depth == 0 {
            return Err(PdfError::MaxDepth);
        }
        let dict = t!(parse_dictionary_object(lexer, r, ctx, max_depth - 1));
        // It might just be the dictionary in front of a stream.
        if t!(lexer.peek()).equals(b"stream") {
            Primitive::Stream(t!(parse_stream_object(dict, lexer, r, ctx)))
        } else {
            Primitive::Dictionary(dict)
        }
    } else if first_lexeme.is_integer() {
        // May be Integer or Reference

        // First backup position
        let pos_bk = lexer.get_pos();

        let second_lexeme = t!(lexer.next());
        if second_lexeme.is_integer() {
            let third_lexeme = t!(lexer.next());
            if third_lexeme.equals(b"R") {
                // It is indeed a reference to an indirect object
                Primitive::Reference(PlainRef {
                    id:  t!(first_lexeme.to::<ObjNr>()),
                    gen: t!(second_lexeme.to::<GenNr>()),
                })
            } else {
                // We are probably in an array of numbers - it's not a reference anyway
                lexer.set_pos(pos_bk as usize); // (roll back the lexer first)
                Primitive::Integer(t!(first_lexeme.to::<i32>()))
            }
        } else {
            // It is but a number
            lexer.set_pos(pos_bk as usize); // (roll back the lexer first)
            Primitive::Integer(t!(first_lexeme.to::<i32>()))
        }
    } else if first_lexeme.is_real_number() {
        // Real Number
        Primitive::Number(t!(first_lexeme.to::<f32>()))
    } else if first_lexeme.starts_with(b"/") {
        // Name
        let s = first_lexeme.reslice(1..).to_string();
        Primitive::Name(s)
    } else if first_lexeme.equals(b"[") {
        if max_depth == 0 {
            return Err(PdfError::MaxDepth);
        }
        let mut array = Vec::new();
        // Array
        loop {
            // Exit if closing delimiter
            if lexer.peek()?.equals(b"]") {
                break;
            }

            let element = t!(parse_with_lexer_ctx(lexer, r, ctx, max_depth - 1));
            array.push(element);
        }
        t!(lexer.next()); // Move beyond closing delimiter

        Primitive::Array(array)
    } else if first_lexeme.equals(b"(") {
        let mut string: Vec<u8> = Vec::new();

        let bytes_traversed = {
            let mut string_lexer = StringLexer::new(lexer.get_remaining_slice());
            for character in string_lexer.iter() {
                string.push(t!(character));
            }
            string_lexer.get_offset() as i64
        };
        // Advance to end of string
        lexer.offset_pos(bytes_traversed as usize);
        // decrypt it
        if let Some(ctx) = ctx {
            string = t!(ctx.decrypt(&mut string)).to_vec();
        }
        Primitive::String(PdfString::new(string))
    } else if first_lexeme.equals(b"<") {
        let mut string: Vec<u8> = Vec::new();

        let bytes_traversed = {
            let mut hex_string_lexer = HexStringLexer::new(lexer.get_remaining_slice());
            for byte in hex_string_lexer.iter() {
                string.push(t!(byte));
            }
            hex_string_lexer.get_offset()
        };
        // Advance to end of string
        lexer.offset_pos(bytes_traversed);

        // decrypt it
        if let Some(ctx) = ctx {
            string = t!(ctx.decrypt(&mut string)).to_vec();
        }
        Primitive::String(PdfString::new(string))
    } else if first_lexeme.equals(b"true") {
        Primitive::Boolean(true)
    } else if first_lexeme.equals(b"false") {
        Primitive::Boolean(false)
    } else if first_lexeme.equals(b"null") {
        Primitive::Null
    } else {
        err!(PdfError::UnknownType {
            pos:          lexer.get_pos(),
            first_lexeme: first_lexeme.to_string(),
            rest:         lexer.read_n(50).to_string(),
        });
    };

    // trace!("Read object"; "Obj" => format!("{}", obj));

    Ok(obj)
}

pub fn parse_stream(
    data: &[u8],
    resolve: &impl Resolve,
    ctx: Option<&Context>,
) -> Result<PdfStream> {
    parse_stream_with_lexer(&mut Lexer::new(data), resolve, ctx)
}

fn parse_stream_with_lexer(
    lexer: &mut Lexer,
    r: &impl Resolve,
    _ctx: Option<&Context>,
) -> Result<PdfStream> {
    let first_lexeme = t!(lexer.next());

    let obj = if first_lexeme.equals(b"<<") {
        let dict = parse_dictionary_object(lexer, r, None, MAX_DEPTH)?;
        // It might just be the dictionary in front of a stream.
        if t!(lexer.peek()).equals(b"stream") {
            t!(parse_stream_object(dict, lexer, r, None))
        } else {
            err!(PdfError::UnexpectedPrimitive {
                expected: "Stream",
                found:    "Dictionary",
            });
        }
    } else {
        err!(PdfError::UnexpectedPrimitive {
            expected: "Stream",
            found:    "something else",
        });
    };

    Ok(obj)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dict_with_empty_name_as_value() {
        use crate::object::NoResolve;

        {
            let data = b"<</App<</Name/>>>>";
            let primitive = super::parse(data, &NoResolve).unwrap();
            let dict = primitive.into_dictionary(&NoResolve).unwrap();

            assert_eq!(dict.len(), 1);
            let app_dict = dict
                .get("App")
                .unwrap()
                .clone()
                .into_dictionary(&NoResolve)
                .unwrap();
            assert_eq!(app_dict.len(), 1);
            let name = app_dict.get("Name").unwrap().as_name().unwrap();
            assert_eq!(name, "");
        }

        {
            let data = b"<</Length 0/App<</Name/>>>>stream\nendstream\n";
            let stream = super::parse_stream(data, &NoResolve, None).unwrap();
            let dict = stream.info;

            assert_eq!(dict.len(), 2);
            let app_dict = dict
                .get("App")
                .unwrap()
                .clone()
                .into_dictionary(&NoResolve)
                .unwrap();
            assert_eq!(app_dict.len(), 1);
            let name = app_dict.get("Name").unwrap().as_name().unwrap();
            assert_eq!(name, "");
        }
    }

    #[test]
    fn dict_with_empty_name_as_key() {
        use crate::object::NoResolve;

        {
            let data = b"<</ true>>";
            let primitive = super::parse(data, &NoResolve).unwrap();
            let dict = primitive.into_dictionary(&NoResolve).unwrap();

            assert_eq!(dict.len(), 1);
            assert_eq!(dict.get("").unwrap().as_bool().unwrap(), true);
        }

        {
            let data = b"<</Length 0/ true>>stream\nendstream\n";
            let stream = super::parse_stream(data, &NoResolve, None).unwrap();
            let dict = stream.info;

            assert_eq!(dict.len(), 2);
            assert_eq!(dict.get("").unwrap().as_bool().unwrap(), true);
        }
    }

    #[test]
    fn empty_array() {
        use crate::object::NoResolve;

        let data = b"[]";
        let primitive = super::parse(data, &NoResolve).unwrap();
        let array = primitive.into_array(&NoResolve).unwrap();
        assert!(array.is_empty());
    }
}
