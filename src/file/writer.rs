use file::{ObjectId, AnyObject};
use err::*;
use std::iter::Iterator;


struct Writer {
    // TODO xref table here
}

impl Writer {
    pub fn new<I>(objects: I) -> Writer
        where I: Iterator<Item=(ObjectId, AnyObject)>
    {
        Writer {
        }
    }

    pub fn write<I>(objects: I) -> Result<()>
        where I: Iterator<Item=(ObjectId, AnyObject)>
    {
        for (id, obj) in objects {
            // TODO write to file, add to xref table
        }
        Ok(())
    }
}
