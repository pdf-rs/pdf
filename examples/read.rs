extern crate pdf;

use std::env::args;
use std::time::SystemTime;

use pdf::file::File;
use pdf::object::*;
use pdf::parser::parse;
use pdf::print_err;

fn main() {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    let file = File::<Vec<u8>>::open(&path).unwrap_or_else(|e| print_err(e));
    let now = SystemTime::now();
    
    let num_pages = file.get_root().pages.count;
    for i in 0..num_pages {
        let _ = file.get_page(i);
    }
    if let Ok(elapsed) = now.elapsed() {
        println!("Time: {}s", elapsed.as_secs() as f64
                 + elapsed.subsec_nanos() as f64 * 1e-9);
    }
}
