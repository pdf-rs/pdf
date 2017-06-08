// Considering whether to impl Object and IndirectObject here.
//

use parser::lexer::*;
use err::*;
use primitive::{Primitive, Stream};
use object::PlainRef;
use parser::{parse_with_lexer, parse_stream_with_lexer};
use object::{GenNr, ObjNr, NO_RESOLVE};


/// Parses an Object starting at the current position of `lexer`. Almost as
/// `Reader::parse_object`, but this function does not take `Reader`, at the expense that it
/// cannot dereference 


pub fn parse_indirect_object(lexer: &mut Lexer) -> Result<(PlainRef, Primitive)> {
    let obj_nr = lexer.next()?.to::<ObjNr>()?;
    let gen_nr = lexer.next()?.to::<GenNr>()?;
    lexer.next_expect("obj")?;

    let obj = parse_with_lexer(lexer)?;

    lexer.next_expect("endobj")?;

    Ok((PlainRef {id: obj_nr, gen: gen_nr}, obj))
}
pub fn parse_indirect_stream(lexer: &mut Lexer) -> Result<(PlainRef, Stream)> {
    let obj_nr = lexer.next()?.to::<ObjNr>()?;
    let gen_nr = lexer.next()?.to::<GenNr>()?;
    lexer.next_expect("obj")?;

    let stm = parse_stream_with_lexer(lexer, NO_RESOLVE)?;

    lexer.next_expect("endobj")?;

    Ok((PlainRef {id: obj_nr, gen: gen_nr}, stm))
}
