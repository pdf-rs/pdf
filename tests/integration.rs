extern crate pdf;
extern crate memmap;

use std::str;
use memmap::Mmap;
use pdf::file::{File, ObjectStream};
use pdf::object::*;
use pdf::parser::parse;

macro_rules! file_path {
    ( $subdir:expr ) => { concat!("tests/files/", $subdir) }
}

const FILES: [&'static str; 3] = [
    "example.pdf",
    "xelatex.pdf",
    "libreoffice.pdf"
];

#[test]
fn open_file() {
    let _ = File::<Vec<u8>>::open(file_path!("example.pdf")).unwrap();
    let _ = File::<Mmap>::open(file_path!("example.pdf")).unwrap();
}

#[test]
fn read_pages() {
    for path in FILES.iter() {
        let path = format!("tests/files/{}", path);
        println!("\n\n == Now testing `{}` ==\n", path);

        let file = File::<Vec<u8>>::open(path.as_str()).unwrap();
        let num_pages = file.get_root().pages.count;
        for i in 0..num_pages {
            println!("\nRead page {}", i);
            let _ = file.get_page(i);
        }
    }
}

#[test]
fn parse_objects_from_stream() {
    let file = File::<Vec<u8>>::open(file_path!("xelatex.pdf")).unwrap();
    let obj_stream = file.deref(Ref::<ObjectStream>::new(PlainRef {id: 13, gen: 0})).unwrap();
    for i in 0..obj_stream.n_objects() {
        let slice = obj_stream.get_object_slice(i).unwrap();
        println!("Object slice #{}: {}\n", i, str::from_utf8(slice).unwrap());
        parse(slice).unwrap();
    }
}

// TODO test decoding
