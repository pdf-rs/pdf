// Considering whether to impl Object and IndirectObject here.
//

use parser::lexer::*;
use err::*;
use primitive::{Primitive, PdfStream};
use parser::{parse_with_lexer, parse_stream_with_lexer};
use object::*;


/// Parses an Object starting at the current position of `lexer`. Almost as
/// `Reader::parse_object`, but this function does not take `Reader`, at the expense that it
/// cannot dereference 


pub fn parse_indirect_object(lexer: &mut Lexer, r: &Resolve) -> Result<(PlainRef, Primitive)> {
    let obj_nr = lexer.next()?.to::<ObjNr>()?;
    let gen_nr = lexer.next()?.to::<GenNr>()?;
    lexer.next_expect("obj")?;

    let obj = parse_with_lexer(lexer, r)?;

    lexer.next_expect("endobj")?;

    Ok((PlainRef {id: obj_nr, gen: gen_nr}, obj))
}
pub fn parse_indirect_stream(lexer: &mut Lexer, r: &Resolve) -> Result<(PlainRef, PdfStream)> {
    let obj_nr = lexer.next()?.to::<ObjNr>()?;
    let gen_nr = lexer.next()?.to::<GenNr>()?;
    lexer.next_expect("obj")?;

    let stm = parse_stream_with_lexer(lexer, r)?;

    lexer.next_expect("endobj")?;

    Ok((PlainRef {id: obj_nr, gen: gen_nr}, stm))
}
