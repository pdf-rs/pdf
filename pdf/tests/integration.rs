extern crate pdf;
extern crate memmap;
extern crate glob;

use std::str;
use memmap::Mmap;
use pdf::file::File;
use pdf::object::*;
use pdf::parser::parse;
use glob::glob;
use pdf::print_err;

macro_rules! file_path {
    ( $subdir:expr ) => { concat!("tests/files/", $subdir) }
}


#[test]
fn open_file() {
    let _ = File::<Vec<u8>>::open(file_path!("example.pdf")).unwrap_or_else(|e| print_err(e));
    let _ = File::<Mmap>::open(file_path!("example.pdf")).unwrap_or_else(|e| print_err(e));
}

#[test]
fn read_pages() {
    for entry in glob("tests/files/*.pdf").expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

                let path = path.to_str().unwrap();
                let file = File::<Vec<u8>>::open(path).unwrap_or_else(|e| print_err(e));
                let num_pages = file.get_root().pages.count;
                for i in 0..num_pages {
                    println!("\nRead page {}", i);
                    let _ = file.get_page(i);
                }
            }
            Err(e) => println!("{:?}", e)
        }
    }
}

#[test]
fn parse_objects_from_stream() {
    use pdf::object::NO_RESOLVE;
    let file = File::<Vec<u8>>::open(file_path!("xelatex.pdf")).unwrap_or_else(|e| print_err(e));
    // .. we know that object 13 of that file is an ObjectStream
    let obj_stream = file.deref(Ref::<ObjectStream>::new(PlainRef {id: 13, gen: 0})).unwrap_or_else(|e| print_err(e));
    for i in 0..obj_stream.n_objects() {
        let slice = obj_stream.get_object_slice(i).unwrap_or_else(|e| print_err(e));
        println!("Object slice #{}: {}\n", i, str::from_utf8(slice).unwrap());
        parse(slice, NO_RESOLVE).unwrap_or_else(|e| print_err(e));
    }
}

// TODO test decoding
