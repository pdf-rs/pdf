//! Basic functionality for parsing a PDF file.

mod lexer;
mod parse_object;
mod parse_xref;

pub use self::lexer::*;
pub use self::parse_object::*;
pub use self::parse_xref::*;

use crate::error::*;
use crate::primitive::{Primitive, Dictionary, PdfStream, PdfString};
use crate::object::{ObjNr, GenNr, PlainRef, Resolve};
use self::lexer::{HexStringLexer, StringLexer};
use crate::crypt::Decoder;

pub struct Context<'a> {
    pub decoder: Option<&'a Decoder>,
    pub obj_nr: u64,
    pub gen_nr: u16
}
impl<'a> Context<'a> {
    pub fn decrypt(&self, mut data: &mut [u8]) {
        if let Some(ref decoder) = self.decoder {
            decoder.decrypt(self.obj_nr, self.gen_nr, &mut data);
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
    parse_with_lexer_ctx(lexer, r, None)
}

/// Recursive. Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is not sufficient.
pub fn parse_with_lexer_ctx(lexer: &mut Lexer, r: &impl Resolve, ctx: Option<&Context>) -> Result<Primitive> {
    let first_lexeme = t!(lexer.next());

    let obj = if first_lexeme.equals(b"<<") {
        let mut dict = Dictionary::default();
        loop {
            // Expect a Name (and Object) or the '>>' delimiter
            let delimiter = t!(lexer.next());
            if delimiter.equals(b"/") {
                let key = lexer.next()?.to_string();
                let obj = t!(parse_with_lexer_ctx(lexer, r, ctx));
                dict.insert(key, obj);
            } else if delimiter.equals(b">>") {
                break;
            } else {
                err!(PdfError::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.to_string(), expected: "/ or >>"});
            }
        }
        // It might just be the dictionary in front of a stream.
        if t!(lexer.peek()).equals(b"stream") {
            t!(lexer.next_stream());

            let length = match dict.get("Length") {
                Some(&Primitive::Integer (n)) => n,
                Some(&Primitive::Reference (n)) => t!(t!(r.resolve(n)).as_integer()),
                _ => err!(PdfError::MissingEntry {field: "Length".into(), typ: "<Stream>"}),
            };

            
            let stream_substr = lexer.read_n(length as usize);
            
            // Finish
            lexer.next_expect("endstream")?;

            let mut data = stream_substr.to_vec();
            
            // decrypt it
            if let Some(ctx) = ctx {
                ctx.decrypt(&mut data);
            }
            
            Primitive::Stream(PdfStream {
                info: dict,
                data,
            })
        } else {
            Primitive::Dictionary (dict)
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
                Primitive::Reference (PlainRef {
                    id: t!(first_lexeme.to::<ObjNr>()),
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
        Primitive::Number (t!(first_lexeme.to::<f32>()))
    } else if first_lexeme.equals(b"/") {
        // Name
        let s = t!(lexer.next()).to_string();
        Primitive::Name(s)
    } else if first_lexeme.equals(b"[") {
        let mut array = Vec::new();
        // Array
        loop {
            let element = t!(parse_with_lexer(lexer, r));
            array.push(element.clone());

            // Exit if closing delimiter
            if lexer.peek()?.equals(b"]") {
                break;
            }
        }
        t!(lexer.next()); // Move beyond closing delimiter

        Primitive::Array (array)
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
            ctx.decrypt(&mut string);
        }
        Primitive::String (PdfString::new(string))
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
            ctx.decrypt(&mut string);
        }
        Primitive::String (PdfString::new(string))
    } else if first_lexeme.equals(b"true") {
        Primitive::Boolean (true)
    } else if first_lexeme.equals(b"false") {
        Primitive::Boolean (false)
    } else if first_lexeme.equals(b"null") {
        Primitive::Null
    } else {
        err!(PdfError::UnknownType {pos: lexer.get_pos(), first_lexeme: first_lexeme.to_string(), rest: lexer.read_n(50).to_string()});
    };

    // trace!("Read object"; "Obj" => format!("{}", obj));

    Ok(obj)
}


pub fn parse_stream(data: &[u8], resolve: &impl Resolve) -> Result<PdfStream> {
    parse_stream_with_lexer(&mut Lexer::new(data), resolve)
}


fn parse_stream_with_lexer(lexer: &mut Lexer, r: &impl Resolve) -> Result<PdfStream> {
    let first_lexeme = t!(lexer.next());

    let obj = if first_lexeme.equals(b"<<") {
        let mut dict = Dictionary::default();
        loop {
            // Expect a Name (and Object) or the '>>' delimiter
            let delimiter = t!(lexer.next());
            if delimiter.equals(b"/") {
                let key = lexer.next()?.to_string();
                let obj = t!(parse_with_lexer(lexer, r));
                dict.insert(key, obj);
            } else if delimiter.equals(b">>") {
                break;
            } else {
                err!(PdfError::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.to_string(), expected: "/ or >>"});
            }
        }
        // It might just be the dictionary in front of a stream.
        if t!(lexer.peek()).equals(b"stream") {
            t!(lexer.next_stream());

            // Get length - look up in `resolve_fn` if necessary
            let length = match dict.get("Length") {
                Some(&Primitive::Reference (reference)) => t!(t!(r.resolve(reference)).as_integer()),
                Some(&Primitive::Integer (n)) => n,
                Some(other) => err!(PdfError::UnexpectedPrimitive {expected: "Integer or Reference", found: other.get_debug_name()}),
                None => err!(PdfError::MissingEntry {typ: "<Dictionary>", field: "Length".into()}),
            };

            
            let stream_substr = lexer.read_n(length as usize);
            // Finish
            t!(lexer.next_expect("endstream"));

            PdfStream {
                info: dict,
                data: stream_substr.to_vec(),
            }
        } else {
            err!(PdfError::UnexpectedPrimitive { expected: "Stream", found: "Dictionary" });
        }
    } else {
        err!(PdfError::UnexpectedPrimitive { expected: "Stream", found: "something else" });
    };

    Ok(obj)
}


