use std;

pub enum Error {
    IO(std::io::Error),
    Other(String),
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
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Error..")
    }
}
