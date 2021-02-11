extern crate pdf;

use std::env::args;
use std::time::SystemTime;
use std::fs;
use std::io::Write;
use std::rc::Rc;
use std::collections::HashMap;

use pdf::file::File;
use pdf::object::*;
use pdf::error::PdfError;


fn main() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    
    let file = File::<Vec<u8>>::open(&path).unwrap();
    for page in file.pages() {
        let page = page.unwrap();
        let resources = page.resources(&file).unwrap();
        if let Some(ref c) = page.contents {
            println!("{}", c);
        }
    }

    Ok(())
}
