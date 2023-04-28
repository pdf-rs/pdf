extern crate pdf;

use std::env::args;
use std::time::SystemTime;
use std::fs;
use std::collections::HashMap;

use pdf::file::{FileOptions};
use pdf::object::*;
use pdf::primitive::Primitive;
use pdf::error::PdfError;
use pdf::enc::StreamFilter;


fn main() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    let now = SystemTime::now();

    let file = FileOptions::cached().open(&path).unwrap();
    if let Some(ref info) = file.trailer.info_dict {
        let title = info.get("Title").and_then(|p| p.to_string_lossy().ok());
        let author = info.get("Author").and_then(|p| p.to_string_lossy().ok());

        let descr = match (title, author) {
            (Some(title), None) => title,
            (None, Some(author)) => format!("[no title] – {}", author),
            (Some(title), Some(author)) => format!("{} – {}", title, author),
            _ => "PDF".into()
        };
        println!("{}", descr);
    }

    let mut images: Vec<_> = vec![];
    let mut fonts = HashMap::new();

    for page in file.pages() {
        let page = page.unwrap();
        let resources = page.resources().unwrap();
        for (i, font) in resources.fonts.values().enumerate() {
            let name = match &font.name {
                Some(name) => name.as_str().into(),
                None => i.to_string(),
            };
            fonts.insert(name, font.clone());
        }
        images.extend(resources.xobjects.iter().map(|(_name, &r)| file.get(r).unwrap())
            .filter(|o| matches!(**o, XObject::Image(_)))
        );
    }

    for (i, o) in images.iter().enumerate() {
        let img = match **o {
            XObject::Image(ref im) => im,
            _ => continue
        };
        let (data, filter) = img.raw_image_data(&file)?;
        let ext = match filter {
            Some(StreamFilter::DCTDecode(_)) => "jpeg",
            Some(StreamFilter::JBIG2Decode) => "jbig2",
            Some(StreamFilter::JPXDecode) => "jp2k",
            Some(StreamFilter::FlateDecode(_)) => "png",
            _ => continue,
        };

        let fname = format!("extracted_image_{}.{}", i, ext);
        
        fs::write(fname.as_str(), data).unwrap();
        println!("Wrote file {}", fname);
    }
    println!("Found {} image(s).", images.len());
    Ok(())
}
