// Considering whether to impl Object and IndirectObject here.
//

use file::Reader;
use file::object::*;
use file::lexer::*;
use err::*;

use inflate::InflateStream;


impl Reader {
    /// Parser an Object from an Object Stream at index `index`.
    pub fn parse_object_from_stream(&self, obj_stream: &Stream, index: u16) -> Result<Object> {
        let _ = obj_stream.dictionary.get("N")?.as_integer()?; /* num object */
        let first = obj_stream.dictionary.get("First")?.as_integer()?;

        let mut lexer = Lexer::new(&obj_stream.content);

        // Just find the byte offset of the one we are interested in
        let mut byte_offset = 0;
        for _ in 0..index+1 {
            lexer.next()?.to::<u32>()?; /* obj_nr. Might want to check whether it's the rigth object. */
            byte_offset = lexer.next()?.to::<u16>()?;
        }

        lexer.set_pos(first as usize + byte_offset as usize);
        self.parse_object(&mut lexer)
    }

    /// Parses an Object starting at the current position of `lexer`.
    pub fn parse_object(&self, lexer: &mut Lexer) -> Result<Object> {
        Reader::parse_object_internal(lexer, Some(self))
    }
    /// Parses an Objec starting at the current position of `lexer`. It
    /// will not follow references when reading the "/Length" of a Stream - but rather return an
    /// `Error`.
    pub fn parse_direct_object(lexer: &mut Lexer) -> Result<Object> {
        Reader::parse_object_internal(lexer, None)
    }

    /// The reason for this is to support two modes of parsing: with and without `&self`. `&self`
    /// is only needed for following references, then especially considering that the "/Length"
    /// entry of a Stream Dictionary may be a Reference.
    fn parse_object_internal(lexer: &mut Lexer, reader: Option<&Reader>) -> Result<Object> {
        let first_lexeme = lexer.next()?;

        let obj = if first_lexeme.equals(b"<<") {
            let mut dict = Dictionary::default();
            loop {
                // Expect a Name (and Object) or the '>>' delimiter
                let delimiter = lexer.next()?;
                if delimiter.equals(b"/") {
                    let key = lexer.next()?.as_string();
                    let obj = Reader::parse_object_internal(lexer, reader)?;
                    dict.set(key, obj);
                } else if delimiter.equals(b">>") {
                    break;
                } else {
                    bail!(ErrorKind::UnexpectedLexeme{ pos: lexer.get_pos(), lexeme: delimiter.as_string(), expected: "/ or >>"});
                }
            }
            // It might just be the dictionary in front of a stream.
            if lexer.peek()?.equals(b"stream") {
                lexer.next()?;

                // Get length
                let length = match reader {
                    Some(reader) => reader.dereference(dict.get("Length")?)?.as_integer()?,
                    None => dict.get("Length")?.as_integer()?,
                };
                // Read the stream
                let mut content = lexer.offset_pos(length as usize).to_vec();
                // Uncompress/decode if there is a filter
                match dict.get("Filter") {
                    Ok(&Object::Name (ref s)) => {
                        if *s == "FlateDecode" {
                            content = Reader::flat_decode(&content);
                        } else {
                            bail!("NOT IMPLEMENTED: Filter type {}", *s);
                        }
                    }
                    Ok(_) => {
                        bail!("NOT IMPLEMENTED: Array of filters");
                    }
                    _ => {}
                }
                // Finish
                lexer.next_expect("endstream")?;

                Object::Stream (Stream {
                    dictionary: dict,
                    content: content,
                })
            } else {
                Object::Dictionary (dict)
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
                    Object::Reference (ObjectId {
                        obj_nr: first_lexeme.to::<u32>()?,
                        gen_nr: second_lexeme.to::<u16>()?,
                    })
                } else {
                    // We are probably in an array of numbers - it's not a reference anyway
                    lexer.set_pos(pos_bk as usize); // (roll back the lexer first)
                    Object::Integer(first_lexeme.to::<i32>()?)
                }
            } else {
                // It is but a number
                lexer.set_pos(pos_bk as usize); // (roll back the lexer first)
                Object::Integer(first_lexeme.to::<i32>()?)
            }
        } else if first_lexeme.is_real_number() {
            // Real Number
            Object::Number (first_lexeme.to::<f32>()?)
        } else if first_lexeme.equals(b"/") {
            // Name
            let s = lexer.next()?.as_string();
            Object::Name(s)
        } else if first_lexeme.equals(b"[") {
            let mut array = Vec::new();
            // Array
            loop {
                let element = Reader::parse_object_internal(lexer, reader)?;
                array.push(element.clone());

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
            lexer.offset_pos(bytes_traversed as usize);

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
    /// Parses an Object starting at the current position of `lexer`. Almost as
    /// `Reader::parse_object`, but this function does not take `Reader`, at the expense that it
    /// cannot dereference 


    pub fn parse_indirect_object(&self, lexer: &mut Lexer) -> Result<IndirectObject> {
        let obj_nr = lexer.next()?.to::<u32>()?;
        let gen_nr = lexer.next()?.to::<u16>()?;
        lexer.next_expect("obj")?;

        let obj = self.parse_object(lexer)?;

        lexer.next_expect("endobj")?;

        Ok(IndirectObject {
            id: ObjectId {obj_nr: obj_nr, gen_nr: gen_nr},
            object: obj,
        })
    }

    // TODO move out to decoding/encoding module
    fn flat_decode(data: &[u8]) -> Vec<u8> {
        let mut inflater = InflateStream::from_zlib();
        let mut out = Vec::<u8>::new();
        let mut n = 0;
        while n < data.len() {
            let res = inflater.update(&data[n..]);
            if let Ok((num_bytes_read, result)) = res {
                n += num_bytes_read;
                out.extend(result);
            } else {
                res.unwrap();
            }
        }
        out
    }

}
