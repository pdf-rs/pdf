extern crate pdf;

use std::env::args;
use std::time::SystemTime;
use std::fs;
use std::collections::HashMap;

use pdf::file::File;
use pdf::object::*;
use pdf::primitive::{Primitive, PdfString};
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
    
    let mut images: Vec<_> = vec![];
    let mut fonts = HashMap::new();
    
    for page in file.pages() {
        let page = page.unwrap();
        let resources = page.resources().unwrap();
        for (i, &font) in resources.fonts.values().enumerate() {
            let font = file.get(font)?;
            let name = match &font.name {
                Some(name) => name.clone(),
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
    
    if let Some(ref forms) = file.get_root().forms {
        println!("Forms:");
        for field in forms.fields.iter() {
            print!("  {:?} = ", field.name);
            match field.value {
                Primitive::String(ref s) => {
                    match s.as_str() {
                        Ok(s) => println!("{:?}", s),
                        Err(_) => println!("{:?}", s),
                    }
                }
                Primitive::Integer(i) => println!("{}", i),
                Primitive::Name(ref s) => println!("{}", s),
                ref p => println!("{:?}", p),
            }
        }
    }

    if let Ok(elapsed) = now.elapsed() {
        println!("Time: {}s", elapsed.as_secs() as f64
                 + elapsed.subsec_nanos() as f64 * 1e-9);
    }
    Ok(())
}
