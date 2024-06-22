//! Basic functionality for parsing a PDF file.

mod lexer;
mod parse_object;
mod parse_xref;

pub use self::lexer::*;
pub use self::parse_object::*;
pub use self::parse_xref::*;

use crate::error::*;
use crate::primitive::StreamInner;
use crate::primitive::{Primitive, Dictionary, PdfStream, PdfString};
use crate::object::{ObjNr, GenNr, PlainRef, Resolve};
use crate::crypt::Decoder;
use bitflags::bitflags;
use istring::{SmallBytes, SmallString, IBytes};

const MAX_DEPTH: usize = 20;


bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ParseFlags: u16 {
        const INTEGER = 1 << 0;
        const STREAM = 1 << 1;
        const DICT = 1 << 2;
        const NUMBER = 1 << 3;
        const NAME = 1 << 4;
        const ARRAY = 1 << 5;
        const STRING = 1 << 6;
        const BOOL = 1 << 7;
        const NULL = 1 << 8;
        const REF = 1 << 9;
        const ANY = (1 << 10) - 1;
    }
}


pub struct Context<'a> {
    pub decoder: Option<&'a Decoder>,
    pub id: PlainRef,
}
impl<'a> Context<'a> {
    pub fn decrypt<'buf>(&self, data: &'buf mut [u8]) -> Result<&'buf [u8]> {
        if let Some(decoder) = self.decoder {
            decoder.decrypt(self.id, data)
        } else {
            Ok(data)
        }
    }
    #[cfg(test)]
    fn fake() -> Self {
        Context {
            decoder: None,
            id: PlainRef { id: 0, gen: 0 }
        }
    }
}

/// Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is insufficient.
pub fn parse(data: &[u8], r: &impl Resolve, flags: ParseFlags) -> Result<Primitive> {
    parse_with_lexer(&mut Lexer::new(data), r, flags)
}

/// Recursive. Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is not sufficient.
pub fn parse_with_lexer(lexer: &mut Lexer, r: &impl Resolve, flags: ParseFlags) -> Result<Primitive> {
    parse_with_lexer_ctx(lexer, r, None, flags, MAX_DEPTH)
}

fn parse_dictionary_object(lexer: &mut Lexer, r: &impl Resolve, ctx: Option<&Context>, max_depth: usize) -> Result<Dictionary> {
    let mut dict = Dictionary::default();
    loop {
        // Expect a Name (and Object) or the '>>' delimiter
        let token = t!(lexer.next());
        if token.starts_with(b"/") {
            let key = token.reslice(1..).to_name()?;
            let obj = t!(parse_with_lexer_ctx(lexer, r, ctx, ParseFlags::ANY, max_depth));
            dict.insert(key, obj);
        } else if token.equals(b">>") {
            break;
        } else {
            err!(PdfError::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: token.to_string(), expected: "/ or >>"});
        }
    }
    Ok(dict)
}

fn parse_stream_object(dict: Dictionary, lexer: &mut Lexer, r: &impl Resolve, ctx: &Context) -> Result<PdfStream> {
    t!(lexer.next_stream());

    let length = match dict.get("Length") {
        Some(&Primitive::Integer(n)) if n >= 0 => n as usize,
        Some(&Primitive::Reference(reference)) => t!(t!(r.resolve_flags(reference, ParseFlags::INTEGER, 1)).as_usize()),
        Some(other) => err!(PdfError::UnexpectedPrimitive { expected: "unsigned Integer or Reference", found: other.get_debug_name() }),
        None => err!(PdfError::MissingEntry { typ: "<Stream>", field: "Length".into() }),
    };

    let stream_substr = lexer.read_n(length);

    if stream_substr.len() != length {
        err!(PdfError::EOF)
    }

    // Finish
    t!(lexer.next_expect("endstream"));

    Ok(PdfStream {
        inner: StreamInner::InFile {
            id: ctx.id,
            file_range: stream_substr.file_range(),
        },
        info: dict,
    })
}

#[inline]
fn check(flags: ParseFlags, allowed: ParseFlags) -> Result<(), PdfError> {
    if !flags.intersects(allowed) {
        return Err(PdfError::PrimitiveNotAllowed { allowed, found: flags });
    }
    Ok(())
}

/// Recursive. Can parse stream but only if its dictionary does not contain indirect references.
/// Use `parse_stream` if this is not sufficient.
pub fn parse_with_lexer_ctx(lexer: &mut Lexer, r: &impl Resolve, ctx: Option<&Context>, flags: ParseFlags, max_depth: usize) -> Result<Primitive> {
    let pos = lexer.get_pos();
    match _parse_with_lexer_ctx(lexer, r, ctx, flags, max_depth) {
        Ok(r) => Ok(r),
        Err(e) => {
            lexer.set_pos(pos);
            Err(e)
        }
    }
}
fn _parse_with_lexer_ctx(lexer: &mut Lexer, r: &impl Resolve, ctx: Option<&Context>, flags: ParseFlags, max_depth: usize) -> Result<Primitive> {

    let input = lexer.get_remaining_slice();
    let first_lexeme = t!(lexer.next(), std::str::from_utf8(input));

    let obj = if first_lexeme.equals(b"<<") {
        check(flags, ParseFlags::DICT)?;

        if max_depth == 0 {
            return Err(PdfError::MaxDepth);
        }
        let dict = t!(parse_dictionary_object(lexer, r, ctx, max_depth-1));
        // It might just be the dictionary in front of a stream.
        if t!(lexer.peek()).equals(b"stream") {
            let ctx = ctx.ok_or(PdfError::PrimitiveNotAllowed { allowed: ParseFlags::STREAM, found: flags })?;
            Primitive::Stream(t!(parse_stream_object(dict, lexer, r, ctx)))
        } else {
            Primitive::Dictionary(dict)
        }
    } else if first_lexeme.is_integer() {
        // May be Integer or Reference
        check(flags, ParseFlags::INTEGER | ParseFlags::REF)?;

        // First backup position
        let pos_bk = lexer.get_pos();

        let second_lexeme = t!(lexer.next());
        if second_lexeme.is_integer() {
            let third_lexeme = t!(lexer.next());
            if third_lexeme.equals(b"R") {
                // It is indeed a reference to an indirect object
                check(flags, ParseFlags::REF)?;
                Primitive::Reference (PlainRef {
                    id: t!(first_lexeme.to::<ObjNr>()),
                    gen: t!(second_lexeme.to::<GenNr>()),
                })
            } else {
                check(flags, ParseFlags::INTEGER)?;
                // We are probably in an array of numbers - it's not a reference anyway
                lexer.set_pos(pos_bk); // (roll back the lexer first)
                Primitive::Integer(t!(first_lexeme.to::<i32>()))
            }
        } else {
            check(flags, ParseFlags::INTEGER)?;
            // It is but a number
            lexer.set_pos(pos_bk); // (roll back the lexer first)
            Primitive::Integer(t!(first_lexeme.to::<i32>()))
        }
    } else if let Some(s) = first_lexeme.real_number() {
        check(flags, ParseFlags::NUMBER)?;
        // Real Number
        Primitive::Number (t!(s.to::<f32>(), s.to_string()))
    } else if first_lexeme.starts_with(b"/") {
        check(flags, ParseFlags::NAME)?;
        // Name

        let mut rest: &[u8] = &first_lexeme.reslice(1..);
        let s = if rest.contains(&b'#') {
            let mut s = IBytes::new();
            while let Some(idx) = rest.iter().position(|&b| b == b'#') {
                use crate::enc::decode_nibble;
                use std::convert::TryInto;
                let [hi, lo]: [u8; 2] = rest.get(idx+1 .. idx+3).ok_or(PdfError::EOF)?.try_into().unwrap();
                let byte = match (decode_nibble(lo), decode_nibble(hi)) {
                    (Some(low), Some(high)) => low | high << 4,
                    _ => return Err(PdfError::HexDecode { pos: idx, bytes: [hi, lo] }),
                };
                s.extend_from_slice(&rest[..idx]);
                s.push(byte);
                rest = &rest[idx+3..];
            }
            s.extend_from_slice(rest);
            SmallBytes::from(s.as_slice())
        } else {
            SmallBytes::from(rest)
        };
        
        Primitive::Name(SmallString::from_utf8(s)?)
    } else if first_lexeme.equals(b"[") {
        check(flags, ParseFlags::ARRAY)?;
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

            let element = t!(parse_with_lexer_ctx(lexer, r, ctx, ParseFlags::ANY, max_depth-1));
            array.push(element);
        }
        t!(lexer.next()); // Move beyond closing delimiter

        Primitive::Array (array)
    } else if first_lexeme.equals(b"(") {
        check(flags, ParseFlags::STRING)?;
        let mut string = IBytes::new();

        let bytes_traversed = {
            let mut string_lexer = StringLexer::new(lexer.get_remaining_slice());
            for character in string_lexer.iter() {
                string.push(t!(character));
            }
            string_lexer.get_offset()
        };
        // Advance to end of string
        lexer.offset_pos(bytes_traversed);
        // decrypt it
        if let Some(ctx) = ctx {
            string = t!(ctx.decrypt(&mut string)).into();
        }
        Primitive::String (PdfString::new(string))
    } else if first_lexeme.equals(b"<") {
        check(flags, ParseFlags::STRING)?;
        let mut string = IBytes::new();

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
            string = t!(ctx.decrypt(&mut string)).into();
        }
        Primitive::String (PdfString::new(string))
    } else if first_lexeme.equals(b"true") {
        check(flags, ParseFlags::BOOL)?;
        Primitive::Boolean (true)
    } else if first_lexeme.equals(b"false") {
        check(flags, ParseFlags::BOOL)?;
        Primitive::Boolean (false)
    } else if first_lexeme.equals(b"null") {
        check(flags, ParseFlags::NULL)?;
        Primitive::Null
    } else {
        err!(PdfError::UnknownType {pos: lexer.get_pos(), first_lexeme: first_lexeme.to_string(), rest: lexer.read_n(50).to_string()});
    };

    // trace!("Read object"; "Obj" => format!("{}", obj));

    Ok(obj)
}


pub fn parse_stream(data: &[u8], resolve: &impl Resolve, ctx: &Context) -> Result<PdfStream> {
    parse_stream_with_lexer(&mut Lexer::new(data), resolve, ctx)
}


fn parse_stream_with_lexer(lexer: &mut Lexer, r: &impl Resolve, ctx: &Context) -> Result<PdfStream> {
    let first_lexeme = t!(lexer.next());

    let obj = if first_lexeme.equals(b"<<") {
        let dict = t!(parse_dictionary_object(lexer, r, None, MAX_DEPTH));
        // It might just be the dictionary in front of a stream.
        if t!(lexer.peek()).equals(b"stream") {
            let ctx = Context {
                decoder: None,
                id: ctx.id
            };
            t!(parse_stream_object(dict, lexer, r, &ctx))
        } else {
            err!(PdfError::UnexpectedPrimitive { expected: "Stream", found: "Dictionary" });
        }
    } else {
        err!(PdfError::UnexpectedPrimitive { expected: "Stream", found: "something else" });
    };

    Ok(obj)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dict_with_empty_name_as_value() {
        use crate::object::NoResolve;
        use super::{ParseFlags, Context};
        {
            let data = b"<</App<</Name/>>>>";
            let primitive = super::parse(data, &NoResolve, ParseFlags::DICT).unwrap();
            let dict = primitive.into_dictionary().unwrap();

            assert_eq!(dict.len(), 1);
            let app_dict = dict.get("App").unwrap().clone().into_dictionary().unwrap();
            assert_eq!(app_dict.len(), 1);
            let name = app_dict.get("Name").unwrap().as_name().unwrap();
            assert_eq!(name, "");
        }

        {
            let data = b"<</Length 0/App<</Name/>>>>stream\nendstream\n";
            let stream = super::parse_stream(data, &NoResolve, &Context::fake()).unwrap();
            let dict = stream.info;

            assert_eq!(dict.len(), 2);
            let app_dict = dict.get("App").unwrap().clone().into_dictionary().unwrap();
            assert_eq!(app_dict.len(), 1);
            let name = app_dict.get("Name").unwrap().as_name().unwrap();
            assert_eq!(name, "");
        }
    }

    #[test]
    fn dict_with_empty_name_as_key() {
        use crate::object::NoResolve;
        use super::{ParseFlags, Context};

        {
            let data = b"<</ true>>";
            let primitive = super::parse(data, &NoResolve, ParseFlags::DICT).unwrap();
            let dict = primitive.into_dictionary().unwrap();

            assert_eq!(dict.len(), 1);
            assert!(dict.get("").unwrap().as_bool().unwrap());
        }

        {
            let data = b"<</Length 0/ true>>stream\nendstream\n";
            let stream = super::parse_stream(data, &NoResolve, &Context::fake()).unwrap();
            let dict = stream.info;

            assert_eq!(dict.len(), 2);
            assert!(dict.get("").unwrap().as_bool().unwrap());
        }
    }

    #[test]
    fn empty_array() {
        use crate::object::NoResolve;
        use super::ParseFlags;

        let data = b"[]";
        let primitive = super::parse(data, &NoResolve, ParseFlags::ARRAY).unwrap();
        let array = primitive.into_array().unwrap();
        assert!(array.is_empty());
    }

    #[test]
    fn compact_array() {
        use crate::object::NoResolve;
        use crate::primitive::{Primitive, PdfString};
        use super::lexer::Lexer;
        use super::*;
        let mut lx = Lexer::new(b"[(Complete L)20(egend for Physical and P)20(olitical Maps)]TJ");
        assert_eq!(parse_with_lexer(&mut lx, &NoResolve, ParseFlags::ANY).unwrap(),
            Primitive::Array(vec![
                Primitive::String(PdfString::new("Complete L".into())),
                Primitive::Integer(20),
                Primitive::String(PdfString::new("egend for Physical and P".into())),
                Primitive::Integer(20),
                Primitive::String(PdfString::new("olitical Maps".into()))
            ])
        );
        assert_eq!(lx.next().unwrap().as_str().unwrap(), "TJ");
        assert!(lx.next().unwrap_err().is_eof());
    }
}
