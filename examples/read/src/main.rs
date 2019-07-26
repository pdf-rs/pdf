extern crate pdf;

use std::env::args;
use std::time::SystemTime;
use std::fs;
use std::io::Write;
use std::rc::Rc;

use pdf::file::File;
use pdf::object::*;
use pdf::error::PdfError;

fn main() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    let now = SystemTime::now();
    let file = File::<Vec<u8>>::open(&path).unwrap();
    
    let mut images: Vec<Rc<_>> = vec![];
    let mut fonts: Vec<Rc<_>> = vec![];
    
    for page in file.pages() {
        let resources = page.as_ref().unwrap().resources(&file).unwrap();
        fonts.extend(resources.fonts.values().cloned());
        images.extend(resources.xobjects.iter()
            .filter_map(|(_, o)| match o { XObject::Image(im) => Some(im.clone()), _ => None })
        );
    }

    for (i,img) in images.iter().enumerate() {
        let fname = format!("extracted_image{}.jpeg", i);
        let mut f = fs::File::create(fname.as_str()).unwrap();
        f.write(&img.data().unwrap()).unwrap();
        println!("Wrote file {}.", fname);
    }
    println!("Found {} image(s).", images.len());


    println!("Found {} font(s).", fonts.len());
    
    if let Ok(elapsed) = now.elapsed() {
        println!("Time: {}s", elapsed.as_secs() as f64
                 + elapsed.subsec_nanos() as f64 * 1e-9);
    }
    Ok(())
}
