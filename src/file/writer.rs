use file::{ObjectId, Object};
use err::*;
use std::iter::Iterator;


struct Writer {
    // TODO xref table here
}

impl Writer {
    pub fn new<I>(objects: I) -> Writer
        where I: Iterator<Item=(ObjectId, Object)>
    {
        Writer {
        }
    }

    pub fn write<I>(objects: I) -> Result<()>
        where I: Iterator<Item=(ObjectId, Object)>
    {
        for (id, obj) in objects {
            // TODO write to file, add to xref table
        }
        Ok(())
    }
}
