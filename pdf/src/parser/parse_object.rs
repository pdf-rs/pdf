// Considering whether to impl Object and IndirectObject here.
//

use crate::parser::lexer::*;
use crate::error::*;
use crate::primitive::{Primitive, PdfStream};
use crate::parser::{parse_with_lexer_ctx, parse_stream_with_lexer, Context};
use crate::object::*;
use crate::crypt::Decoder;

/// Parses an Object starting at the current position of `lexer`. Almost as
/// `Reader::parse_object`, but this function does not take `Reader`, at the expense that it
/// cannot dereference 

pub fn parse_indirect_object(lexer: &mut Lexer, r: &impl Resolve, decoder: Option<&Decoder>) -> Result<(PlainRef, Primitive)> {
    let obj_nr = t!(lexer.next()).to::<ObjNr>()?;
    let gen_nr = t!(lexer.next()).to::<GenNr>()?;
    lexer.next_expect("obj")?;

    let ctx = Context {
        decoder,
        obj_nr,
        gen_nr
    };
    let obj = t!(parse_with_lexer_ctx(lexer, r, Some(&ctx)));

    t!(lexer.next_expect("endobj"));

    Ok((PlainRef {id: obj_nr, gen: gen_nr}, obj))
}
pub fn parse_indirect_stream(lexer: &mut Lexer, r: &impl Resolve, decoder: Option<&Decoder>) -> Result<(PlainRef, PdfStream)> {
    let obj_nr = t!(t!(lexer.next()).to::<ObjNr>());
    let gen_nr = t!(t!(lexer.next()).to::<GenNr>());
    lexer.next_expect("obj")?;

    let ctx = Context {
        decoder,
        obj_nr,
        gen_nr
    };
    let stm = t!(parse_stream_with_lexer(lexer, r, Some(&ctx)), obj_nr, gen_nr);

    t!(lexer.next_expect("endobj"));

    Ok((PlainRef {id: obj_nr, gen: gen_nr}, stm))
}
