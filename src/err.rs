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
        // FromUtf8(::std::string::FromUtf8Error);
    }
    // Define additional `ErrorKind` variants. The syntax here is
    // the same as `quick_error!`, but the `from()` and `cause()`
    // syntax is not supported.
    errors {
        EOF {
            description("Unexpected end of file")
            display("Unexpected end of file")
        }
        InvalidXref {pos: usize} {
            description("Invalid Xref")
            display("Invalid Xref at byte {}", pos)
        }
        ParseError {word: String} {
            description("Parse error")
            display("Parse error - word: {}", word)
        }
        UnexpectedLexeme {pos: usize, lexeme: String, expected: &'static str} {
            description("Unexpected token.")
            display("Unexpected token '{}' at {} - expected '{}'", lexeme, pos, expected)
        }
        NotFound {word: String} {
            description("Word not found.")
            display("'{}' not found.", word)
        }
        EntryNotFound{key: &'static str} {
            description("Entry not found.")
            display("'{}' not found in dictionary", key)
        }
        FreeObject {obj_nr: u64} {
            description("Tried to dereference free object.")
            display("Tried to dereference free object nr {}.", obj_nr)
        }
        WrongObjectType {expected: &'static str, found: &'static str} {
            description("Function called on object of wrong type.")
            display("Expected {}, found {}.", expected, found)
        }
        /// Page out of bounds / doesn't exist
        OutOfBounds {
            description("Page out of bounds.")
            display("Page out of bounds.")
        }

        /// Cannot follow reference during parsing (most likely /Length of Stream)
        FollowReference {
            description("Cannot follow reference during parsing.")
            display("Cannot follow reference during parsing.")
        }
    }
}
