use file::{ObjectId, Primitive};
use err::*;
use std::iter::Iterator;


struct Writer {
    // TODO xref table here
}

impl Writer {
    pub fn new<I>(objects: I) -> Writer
        where I: Iterator<Item=(ObjectId, Primitive)>
    {
        Writer {
        }
    }

    pub fn write<I>(objects: I) -> Result<()>
        where I: Iterator<Item=(ObjectId, Primitive)>
    {
        for (id, obj) in objects {
            // TODO write to file, add to xref table
        }
        Ok(())
    }
}
