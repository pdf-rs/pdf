extern crate pdf;

use std::collections::HashMap;
use std::env::args;
use std::fs;
use std::time::SystemTime;

use pdf::enc::StreamFilter;
use pdf::error::PdfError;
use pdf::file::{FileOptions, Log};
use pdf::object::*;
use pdf::primitive::Primitive;

struct VerboseLog;
impl Log for VerboseLog {
    fn load_object(&self, r: PlainRef) {
        println!("load {r:?}");
    }
    fn log_get(&self, r: PlainRef) {
        println!("get {r:?}");
    }
}

#[cfg(feature = "cache")]
fn main() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);
    let now = SystemTime::now();

    let file = FileOptions::cached().log(VerboseLog).open(&path).unwrap();
    let resolver = file.resolver();

    if let Some(ref info) = file.trailer.info_dict {
        let title = info.title.as_ref().map(|p| p.to_string_lossy());
        let author = info.author.as_ref().map(|p| p.to_string_lossy());

        let descr = match (title, author) {
            (Some(title), None) => title,
            (None, Some(author)) => format!("[no title] – {}", author),
            (Some(title), Some(author)) => format!("{} – {}", title, author),
            _ => "PDF".into(),
        };
        println!("{}", descr);
    }

    let mut images: Vec<_> = vec![];
    let mut fonts = HashMap::new();

    for page in file.pages() {
        let page = page.unwrap();
        let resources = page.resources().unwrap();
        for (i, font) in resources.fonts.values().enumerate() {
            let font = font.load(&resolver)?;
            let name = match &font.name {
                Some(name) => name.as_str().into(),
                None => i.to_string(),
            };
            fonts.insert(name, font.clone());
        }
        images.extend(
            resources
                .xobjects
                .iter()
                .map(|(_name, &r)| resolver.get(r).unwrap())
                .filter(|o| matches!(**o, XObject::Image(_))),
        );
    }

    for (i, o) in images.iter().enumerate() {
        let img = match **o {
            XObject::Image(ref im) => im,
            _ => continue,
        };
        let (mut data, filter) = img.raw_image_data(&resolver)?;
        let ext = match filter {
            Some(StreamFilter::DCTDecode(_)) => "jpeg",
            Some(StreamFilter::JBIG2Decode(_)) => "jbig2",
            Some(StreamFilter::JPXDecode) => "jp2k",
            Some(StreamFilter::FlateDecode(_)) => "png",
            Some(StreamFilter::CCITTFaxDecode(_)) => {
                data = fax::tiff::wrap(&data, img.width, img.height).into();
                "tiff"
            }
            _ => continue,
        };

        let fname = format!("extracted_image_{}.{}", i, ext);

        fs::write(fname.as_str(), data).unwrap();
        println!("Wrote file {}", fname);
    }
    println!("Found {} image(s).", images.len());

    for (name, font) in fonts.iter() {
        let fname = format!("font_{}", name);
        if let Some(Ok(data)) = font.embedded_data(&resolver) {
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
                Primitive::String(ref s) => println!("{}", s.to_string_lossy()),
                Primitive::Integer(i) => println!("{}", i),
                Primitive::Name(ref s) => println!("{}", s),
                ref p => println!("{:?}", p),
            }
        }
    }

    if let Ok(elapsed) = now.elapsed() {
        println!(
            "Time: {}s",
            elapsed.as_secs() as f64 + elapsed.subsec_nanos() as f64 * 1e-9
        );
    }
    Ok(())
}
