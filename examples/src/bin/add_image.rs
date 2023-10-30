use std::{path::PathBuf, io::BufReader, fs::File, error::Error};

use pdf::{
    error::PdfError,
    file::FileOptions,
    object::*,
    build::*,
    primitive::{PdfString, Name}, enc::{StreamFilter, DCTDecodeParams}, content::{Op, Matrix, Content},
};

use clap::Parser;
use std::io::Cursor;
use image::io::Reader as ImageReader;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file
    #[arg(short, long)]
    input: PathBuf,

    /// Page number
    #[arg(long)]
    image: PathBuf,

    /// Page number to add the image to
    #[arg(short, long, default_value_t = 0)]
    page: u32,

    /// Output file
    #[arg(short, long)]
    output: PathBuf,
}

struct Point {
    x: f32,
    y: f32
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    
    let img_data = std::fs::read(&args.image)?;
    let img = ImageReader::new(Cursor::new(&img_data)).with_guessed_format()?.decode()?;
    let image_dict = ImageDict {
        width: img.width(),
        height: img.height(),
        color_space: Some(ColorSpace::DeviceRGB),
        bits_per_component: Some(8),
        .. Default::default()
    };
    let image = Stream::new_with_filters(image_dict, img_data, vec![StreamFilter::DCTDecode(DCTDecodeParams { color_transform: None})]);

    let mut file = FileOptions::cached().open(&args.input).unwrap();
    let page = file.get_page(args.page).expect("no such page");

    let resources = page.resources()?;
    let mut resources2: Resources = (**resources).clone();

    let image_obj = XObject::Image(ImageXObject { inner: image });
    let image_ref = file.create(image_obj)?;

    // assume that name did not exist
    let image_name = Name::from("MyImage");
    resources2.xobjects.insert(image_name.clone(), image_ref.get_ref());


    let mut ops = page.contents.as_ref().unwrap().operations(&file.resolver())?;

    let scale = Point { x: img.width() as f32, y: img.height() as f32 };
    let skew = Point { x: 0.0, y: 0.0 };
    let position = Point { x: 100., y: 100. };

    ops.append(&mut vec![
        Op::Save, // ADD IMAGE START
        Op::Transform { matrix: Matrix{ // IMAGE MANIPULATION
            a: scale.x * 0.1, d: scale.y * 0.1,
            b: skew.x, c: skew.y,
            e: position.x, f: position.y,
        } },
        Op::XObject {name: image_name}, // IMAGE
        Op::Restore, // ADD IMAGE STOP
    ]);

    let mut page2: Page = (*page).clone();
    page2.contents = Some(Content::from_ops(ops));
    page2.resources = Some(file.create(resources2)?.into());

    file.update(page.get_ref().get_inner(), page2)?;

    file.save_to(&args.output)?;

    Ok(())
}
