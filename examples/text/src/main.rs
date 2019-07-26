extern crate pdf;

use std::env::args;

use pdf::file::File;
use pdf::content::*;
use pdf::primitive::Primitive;

fn add_primitive(p: &Primitive, out: &mut String) {
    // println!("p: {:?}", p);
    match p {
        &Primitive::String(ref s) => if let Ok(text) = s.as_str() {
            out.push_str(text);
        }
        &Primitive::Array(ref a) => for p in a.iter() {
            add_primitive(p, out);
        }
        _ => ()
    }
}

fn main() {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    let file = File::<Vec<u8>>::open(&path).unwrap();
    
    let mut out = String::new();
    for page in file.pages() {
        for Operation { ref operator, ref operands } in &page.unwrap().contents.as_ref().unwrap().operations {
            // println!("{} {:?}", operator, operands);
            match operator.as_str() {
                "Tj" | "TJ" | "BT" => operands.iter().for_each(|p| add_primitive(p, &mut out)),
                _ => {}
            }
        }
    }
    println!("{}", out);
}
