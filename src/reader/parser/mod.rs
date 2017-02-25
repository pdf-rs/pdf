// Considering whether to impl Object and IndirectObject here.
//

mod xref_stream;

use reader::PdfReader;
use object::*;
use xref::*;
use reader::lexer::*;
use err::*;

use inflate::InflateStream;
use std::io::SeekFrom;


// Note: part of `impl` is in xref_stream module.
impl PdfReader {
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

    pub fn parse_object(&self, lexer: &mut Lexer) -> Result<Object> {
        let first_lexeme = lexer.next()?;

        let obj = if first_lexeme.equals(b"<<") {
            let mut dict = Dictionary::new();
            loop {
                // Expect a Name (and Object) or the '>>' delimiter
                let delimiter = lexer.next()?;
                if delimiter.equals(b"/") {
                    let key = lexer.next()?.as_string();
                    let obj = self.parse_object(lexer)?;
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

                println!("DEBUG.. Dictionary length = {}", dict.get("Length")?);
                // Get length
                let length = { dict.get("Length")?.as_integer()? };
                // Read the stream
                let mut content = lexer.offset_pos(length as usize).to_vec();
                // Uncompress/decode if there is a filter
                match dict.get("Filter") {
                    Ok(&Object::Name (ref s)) => {
                        if *s == "FlateDecode".to_string() {
                            content = PdfReader::flat_decode(&content);
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
            Object::RealNumber (first_lexeme.to::<f32>()?)
        } else if first_lexeme.equals(b"/") {
            // Name
            let s = lexer.next()?.as_string();
            Object::Name(s)
        } else if first_lexeme.equals(b"[") {
            let mut array = Vec::new();
            // Array
            loop {
                let element = self.parse_object(lexer)?;
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



    /// Reads xref sections (from stream) and trailer starting at the position of the Lexer.
    pub fn parse_xref_stream_and_trailer(&self, lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
        let xref_stream = self.parse_indirect_object(lexer).chain_err(|| "Reading Xref stream")?.object.as_stream()?;

        // Get 'W' as array of integers
        let width = xref_stream.dictionary.get("W")?.borrow_integer_array()?;
        let num_entries = xref_stream.dictionary.get("Size")?.as_integer()?;

        let indices: Vec<(i32, i32)> = {
            match xref_stream.dictionary.get("Index") {
                Ok(obj) => obj.borrow_integer_array()?,
                Err(_) => vec![0, num_entries],
            }.chunks(2).map(|c| (c[0], c[1])).collect()
            // ^^ TODO panics if odd number of elements - how to handle it?
        };
        
        let (dict, data) = (xref_stream.dictionary, xref_stream.content);
        
        let mut data_left = &data[..];

        let mut sections = Vec::new();
        for (first_id, num_objects) in indices {
            let section = PdfReader::parse_xref_section_from_stream(first_id, num_objects, &width, &mut data_left)?;
            sections.push(section);
        }
        // debug!("Xref stream"; "Sections" => format!("{:?}", sections));

        Ok((sections, dict))
    }

    /// Reads xref sections (from table) and trailer starting at the position of the Lexer.
    pub fn parse_xref_table_and_trailer(&self, lexer: &mut Lexer) -> Result<(Vec<XrefSection>, Dictionary)> {
        let mut sections = Vec::new();
        
        // Keep reading subsections until we hit `trailer`
        while !lexer.peek()?.equals(b"trailer") {
            let start_id = lexer.next_as::<u32>()?;
            let num_ids = lexer.next_as::<u32>()?;

            let mut section = XrefSection::new(start_id);

            for _ in 0..num_ids {
                let w1 = lexer.next()?;
                let w2 = lexer.next()?;
                let w3 = lexer.next()?;
                if w3.equals(b"f") {
                    section.add_free_entry(w1.to::<u32>()?, w2.to::<u16>()?);
                } else if w3.equals(b"n") {
                    section.add_inuse_entry(w1.to::<usize>()?, w2.to::<u16>()?);
                } else {
                    bail!(ErrorKind::UnexpectedLexeme {pos: lexer.get_pos(), lexeme: w3.as_string(), expected: "f or n"});
                }
            }
            sections.push(section);
        }
        // Read trailer
        lexer.next_expect("trailer")?;
        let trailer = self.parse_object(lexer)?.as_dictionary()?;
     
        Ok((sections, trailer))
    }

    // TODO move out to decoding/encoding module
    fn flat_decode(data: &Vec<u8>) -> Vec<u8> {
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
