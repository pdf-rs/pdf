extern crate pdf;

use std::env::args;
use pdf::file::File;
use pdf::primitive::{PdfString, Primitive};

fn walk_node() {}

fn main() {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    
    let file = File::<Vec<u8>>::open(&path).unwrap();
    if let Some(ref names) = file.get_root().names {
        let mut count = 0;
        let mut cb = |key: &PdfString, val: &Primitive| {
            println!("{:?} {:?}", key, val);
            count += 1;
        };
        if let Some(ref pages) = names.pages {
            pages.walk(&file, &mut cb);
        }
        if let Some(ref dests) = names.dests {
            dests.walk(&file, &mut cb);
        }
        println!("{} items", count);
    }
}
