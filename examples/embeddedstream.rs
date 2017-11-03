extern crate pdf;
extern crate glob;

use std::str;
use std::path::Path;
use std::io::Write;
use std::fs;
use pdf::file::File;
use pdf::object::*;
use pdf::parser::parse;
use glob::glob;
use pdf::print_err;

macro_rules! file_path {
    ( $subdir:expr ) => { concat!("tests/files/", $subdir) }
}


fn main() {
    let path = "tests/files/jpeg.pdf";
    let file = File::<Vec<u8>>::open(path).unwrap_or_else(|e| print_err(e));
    let mut images = file.get_images();
    for (i, img) in images.iter().enumerate() {
        let fname = format!("extracted_image{}.jpeg", i);
        let mut f = fs::File::create(fname.as_str()).unwrap();
        f.write(&img.data).unwrap();
        println!("Saved file {}", fname);
    }

    // The following is just a test to see which files have namedicts. None of them have it.
    // for entry in glob("tests/files/*.pdf").expect("Failed to read glob pattern") {
        // match entry {
            // Ok(path) => {
                // println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

                // let file = File::<Vec<u8>>::open(path.to_str().unwrap()).unwrap_or_else(|e| print_err(e));
                // match file.get_root().names {
                    // Some(_) => println!("Has name dict"),
                    // None => println!("No name dict")
                // }


            // }
            // Err(e) => println!("{:?}", e)
        // }
    // }
}
