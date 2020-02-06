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
    println!("read: {}", path);
    let now = SystemTime::now();
    
    let file = File::<Vec<u8>>::open(&path).unwrap();
    if let Some(ref info) = file.trailer.info_dict {
        let title = info.get("Title").and_then(|p| p.as_str());
        let author = info.get("Author").and_then(|p| p.as_str());

        let descr = match (title, author) {
            (Some(title), None) => title.into(),
            (None, Some(author)) => format!("[no title] – {}", author),
            (Some(title), Some(author)) => format!("{} – {}", title, author),
            _ => "PDF".into()
        };
        println!("{}", descr);
    }
    
    let mut images: Vec<Rc<_>> = vec![];
    let mut fonts = HashMap::new();
    
    for page in file.pages() {
        let resources = page.as_ref().unwrap().resources(&file).unwrap();
        for font in resources.fonts.values() {
            fonts.insert(font.name.clone(), font.clone());
        }
        images.extend(resources.xobjects.iter()
            .filter_map(|(_, o)| match o { XObject::Image(im) => Some(im.clone()), _ => None })
        );
    }

    for (i,img) in images.iter().enumerate() {
        let fname = format!("extracted_image_{}.jpeg", i);
        if let Some(data) = img.as_jpeg() {
            fs::write(fname.as_str(), data).unwrap();
            println!("Wrote file {}", fname);
        }
    }
    println!("Found {} image(s).", images.len());


    for (name, font) in fonts.iter() {
        let fname = format!("font_{}", name);
        if let Some(Ok(data)) = font.embedded_data() {
            fs::write(fname.as_str(), data).unwrap();
            println!("Wrote file {}", fname);
        }
    }
    println!("Found {} font(s).", fonts.len());
    
    if let Ok(elapsed) = now.elapsed() {
        println!("Time: {}s", elapsed.as_secs() as f64
                 + elapsed.subsec_nanos() as f64 * 1e-9);
    }
    Ok(())
}
