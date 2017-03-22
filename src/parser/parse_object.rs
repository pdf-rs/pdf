// Considering whether to impl Object and IndirectObject here.
//

use parser::lexer::*;
use err::*;
use primitive::{Primitive, Dictionary, Stream};
use object::PlainRef;
use file::ObjectStream;
use parser::{parse_with_lexer, parse_stream_with_lexer, parse};
use object::{GenNr, ObjNr};

use inflate::InflateStream;

use std::io;


/// Parser an Object from an Object Stream at index `index`.
pub fn parse_object_from_stream<'a, W: io::Write + 'a>(obj_stream: &ObjectStream<W>, index: u16) -> Result<Primitive> {
    let _ = obj_stream.info.n; /* num object */
    let first = obj_stream.info.first;

    let mut lexer = Lexer::new(&obj_stream.data);

    // Just find the byte offset of the one we are interested in
    let mut byte_offset = 0;
    for _ in 0..index+1 {
        lexer.next()?.to::<u32>()?; /* obj_nr. Might want to check whether it's the rigth object. */
        byte_offset = lexer.next()?.to::<u16>()?;
    }

    // lexer.set_pos(first as usize + byte_offset as usize);
    let obj_start = first as usize + byte_offset as usize;
    parse(&obj_stream.data[obj_start..])
}

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
pub fn parse_indirect_stream<'a>(lexer: &mut Lexer<'a>) -> Result<(PlainRef, Stream<'a>)> {
    let obj_nr = lexer.next()?.to::<ObjNr>()?;
    let gen_nr = lexer.next()?.to::<GenNr>()?;
    lexer.next_expect("obj")?;

    let stm = parse_stream_with_lexer(lexer, &|plain_ref| Err(ErrorKind::FollowReference.into()))?;

    lexer.next_expect("endobj")?;

    Ok((PlainRef {id: obj_nr, gen: gen_nr}, stm))
}
