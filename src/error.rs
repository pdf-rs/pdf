use std;
use std::result;
use std::error;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    Other(String),

    // Pdf errors:
    EOF,
    InvalidXref {pos: usize},
    ParseError {word: String},
    UnexpectedToken {pos: usize, token: String, expected: &'static str},
    UnexpectedType {pos: usize},
    NotFound {word: String},
    FreeObject {obj_nr: i32},
}


impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IO(e)
    }
}
impl From<String> for Error {
    fn from(e: String) -> Error {
        Error::Other(e)
    }
}
impl From<&'static str> for Error {
    fn from(e: &'static str) -> Error {
        Error::Other(e.to_string())
    }
}


impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self, f);
        Ok(())
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match *self  {
            Error::EOF => "EOF",
            Error::InvalidXref{pos:_} => "Invalid Xref",
            Error::ParseError{word:_} => "Parse error",
            Error::UnexpectedToken{pos:_, token:_, expected: _} => "Unexpected entry in dictionary (expected name or close delimiter).",
            Error::NotFound{word:_} => "Word not found.",
            Error::UnexpectedType{pos:_} => "Expected integer.",
            Error::FreeObject{obj_nr:_} => "Tried to dereference free object.",

            Error::Other(ref desc) => &desc,
            Error::IO(ref err) => err.description(),
        }
    }
}
