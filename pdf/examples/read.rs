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

/// Turn one image XObject into bytes some other program can open.
///
/// JPEG, JPEG 2000, JBIG2 and CCITT images are already in a standalone format
/// (CCITT is wrapped in a minimal TIFF, which is what `fax` is for). Anything
/// else is decoded to raw samples, and raw samples on their own are not an
/// image file: they need a header. For those we write a PNG (issue #206).
fn image_file(
    img: &ImageXObject,
    resolver: &impl Resolve,
) -> Result<Option<(&'static str, Vec<u8>)>, String> {
    let (data, filter) = img.raw_image_data(resolver).map_err(|e| e.to_string())?;
    Ok(match filter {
        Some(StreamFilter::DCTDecode(_)) => Some(("jpeg", data.to_vec())),
        Some(StreamFilter::JPXDecode) => Some(("jp2k", data.to_vec())),
        Some(StreamFilter::JBIG2Decode(_)) => Some(("jbig2", data.to_vec())),
        Some(StreamFilter::CCITTFaxDecode(_)) => {
            Some(("tiff", fax::tiff::wrap(&data, img.width, img.height).into()))
        }
        Some(StreamFilter::FlateDecode(_)) | Some(StreamFilter::BrotliDecode(_)) | None => {
            // `raw_image_data` leaves Flate/Brotli compressed on purpose (for
            // PDF producers that wrap PNG in Flate); `image_data` decodes all the
            // way to samples, which is what a PNG needs.
            let _ = data;
            let samples = img.image_data(resolver).map_err(|e| e.to_string())?;
            match png_of(img, &samples)? {
                Some(png) => Some(("png", png)),
                None => None,
            }
        }
        Some(other) => {
            eprintln!("  (don't know how to save an image with filter {other:?})");
            None
        }
    })
}

/// Wrap raw decoded samples in a PNG. `Ok(None)` when the colour space or bit
/// depth has no PNG equivalent we handle.
fn png_of(img: &ImageXObject, data: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let bits = img.bits_per_component.unwrap_or(8);
    let depth = match bits {
        1 => png::BitDepth::One,
        2 => png::BitDepth::Two,
        4 => png::BitDepth::Four,
        8 => png::BitDepth::Eight,
        16 => png::BitDepth::Sixteen,
        n => {
            eprintln!("  (unsupported bits per component: {n})");
            return Ok(None);
        }
    };

    let mut palette: Option<Vec<u8>> = None;
    let color = if img.image_mask {
        png::ColorType::Grayscale
    } else {
        match &img.color_space {
            None | Some(ColorSpace::DeviceGray) | Some(ColorSpace::CalGray(_)) => {
                png::ColorType::Grayscale
            }
            Some(ColorSpace::DeviceRGB) | Some(ColorSpace::CalRGB(_)) => png::ColorType::Rgb,
            Some(ColorSpace::Indexed(base, _, lookup)) => {
                // PNG stores the palette as RGB triples.
                let base = match **base {
                    ColorSpace::DeviceRGB | ColorSpace::CalRGB(_) => 3,
                    ColorSpace::DeviceGray | ColorSpace::CalGray(_) => 1,
                    _ => {
                        eprintln!("  (indexed colour space with a non-RGB base: {base:?})");
                        return Ok(None);
                    }
                };
                let mut pal = Vec::with_capacity(lookup.len() / base * 3);
                for entry in lookup.chunks(base) {
                    match (base, entry) {
                        (3, [r, g, b]) => pal.extend_from_slice(&[*r, *g, *b]),
                        (1, [g]) => pal.extend_from_slice(&[*g, *g, *g]),
                        _ => break,
                    }
                }
                // the palette is rounded up to a whole number of colours
                while pal.len() % 3 != 0 {
                    pal.push(0);
                }
                palette = Some(pal);
                png::ColorType::Indexed
            }
            other => {
                eprintln!("  (no PNG for colour space {other:?})");
                return Ok(None);
            }
        }
    };

    // CMYK samples are not a PNG colour type: convert to 8 bit RGB.
    let samples: Vec<u8> = if !img.image_mask
        && matches!(
            img.color_space,
            Some(ColorSpace::DeviceCMYK) | Some(ColorSpace::CalCMYK(_))
        )
    {
        if bits != 8 {
            eprintln!("  (CMYK image with {bits} bits per component)");
            return Ok(None);
        }
        data.chunks(4)
            .flat_map(|px| {
                let k = 255 - px.get(3).copied().unwrap_or(0) as u32;
                let conv = |c: u8| ((255 - c as u32) * k / 255) as u8;
                [conv(px[0]), conv(px[1]), conv(px[2])]
            })
            .collect()
    } else {
        data.to_vec()
    };

    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width, img.height);
        enc.set_color(color);
        enc.set_depth(depth);
        if let Some(pal) = palette.as_deref() {
            enc.set_palette(pal);
        }
        let mut writer = enc
            .write_header()
            .map_err(|e| format!("png header: {e}"))?;
        writer
            .write_image_data(&samples)
            .map_err(|e| format!("png data: {e}"))?;
        writer.finish().map_err(|e| format!("png: {e}"))?;
    }
    Ok(Some(out))
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
        let (ext, data) = match image_file(img, &resolver) {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("  image {}: {e}", i + 1);
                continue;
            }
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
