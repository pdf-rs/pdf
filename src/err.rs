use object::ObjNr;
error_chain! {
    // The type defined for this error. These are the conventional
    // and recommended names, but they can be arbitrarily chosen.
    // It is also possible to leave this block out entirely, or
    // leave it empty, and these names will be used automatically.
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    // Automatic conversions between this error chain and other
    // error types not defined by the `error_chain!`. These will be
    // wrapped in a new error with, in this case, the
    // `ErrorKind::Temp` variant. The description and cause will
    // forward to the description and cause of the original error.
    //
    // Optionally, some attributes can be added to a variant.
    foreign_links {
        Io(::std::io::Error);
        Utf8(::std::str::Utf8Error);
        StringUtf8(::std::string::FromUtf8Error);
        ParseInt(::std::num::ParseIntError);
    }
    // Define additional `ErrorKind` variants. The syntax here is
    // the same as `quick_error!`, but the `from()` and `cause()`
    // syntax is not supported.
    errors {
        ///////////////////
        // Syntax / parsing
        
        EOF {
            description("Unexpected end of file")
            display("Unexpected end of file")
        }
        FromStrError {word: String} { /* TODO: can be avoided if figure out how to have foreign link on F::Err in str::parse<F>() */
            description("Error parsing from string")
            display("Error parsing from string - word: {}", word)
        }
        UnexpectedLexeme {pos: usize, lexeme: String, expected: &'static str} {
            description("Unexpected token.")
            display("Unexpected token '{}' at {} - expected '{}'", lexeme, pos, expected)
        }
        UnknownType {pos: usize, first_lexeme: String, rest: String} {
            // (kinda the same as the above, but 'rest' may be useful)
            description("Unexpected ")
            display("Expecting an object, encountered {} at pos {}. Rest:\n{}\n\n((end rest))", first_lexeme, pos, rest)
        }
        NotFound {word: String} {
            description("Word not found.")
            display("'{}' not found.", word)
        }
        FollowReference {
            description("Cannot follow reference during parsing (most likely /Length of Stream).")
            display("Cannot follow reference during parsing - no resolve fn given (most likely /Length of Stream).")
        }
        XRefStreamType {found: u64} {
            description("Erroneous 'type' field in xref stream - expected 0, 1 or 2")
            display("Erroneous 'type' field in xref stream - expected 0, 1 or 2, found {}", found)
        }
        ContentReadPastBoundary {
            description("Parsing read past boundary of Contents.")
        }
        //////////////////
        // Encode/decode
        HexDecode {pos: usize, bytes: [u8; 2]} {
            description("Hex decode error")
            display("Hex decode error. Position {}, bytes {}, {}", pos, bytes[0], bytes[1])
        }
        Ascii85TailError  {
            description("Ascii85 tail error")
        }
        IncorrectPredictorType {n: u8} {
            description("Failed to convert u8 into PredictorType")
            display("Failed to convert '{}' into PredictorType", n)
        }
        //////////////////
        // Dictionary
        EntryNotFound{key: &'static str} {
            description("Dictionary entry not found.")
            display("'{}' not found in dictionary.", key)
        }
        WrongDictionaryType {expected: String, found: String} {
            display("Expected dictionary /Type = {}. Found /Type = {}.", expected, found)
        }
        //////////////////
        // Misc
        FreeObject {obj_nr: u64} {
            description("Tried to dereference free object.")
            display("Tried to dereference free object nr {}.", obj_nr)
        }
        NullRef {obj_nr: u64} {
            description("Tried to dereference non-existing object.")
            display("Tried to dereference non-existing object nr {}.", obj_nr)
        }

        UnexpectedPrimitive {expected: &'static str, found: &'static str} {
            description("Expected a certain primitive kind, found another.")
            display("Expected {}, found {}.", expected, found)
        }
        /*
        WrongObjectType {expected: &'static str, found: &'static str} {
            description("Function called on object of wrong type.")
            display("Expected {}, found {}.", expected, found)
        }
        */
        ObjStmOutOfBounds {index: usize, max: usize} {
            description("Object stream index out of bounds.")
            display("Object stream index out of bounds ({}/{}).", index, max)
        }
        PageOutOfBounds {page_nr: i32, max: i32} {
            description("Page out of bounds.")
            display("Page out of bounds ({}/{}).", page_nr, max)
        }
        PageNotFound {page_nr: i32} {
            description("The page requested could not be found in the page tree.")
            display("Page {} could not be found in the page tree.", page_nr)
        }
        UnspecifiedXRefEntry {id: ObjNr} {
            description("Entry in xref table unspecified")
            display("Entry {} in xref table unspecified", id)
        }


    }
}
