use std::{error::Error, path::PathBuf};

use pdf::{
    content::{Content, Matrix, Op},
    enc::{DCTDecodeParams, StreamFilter},
    file::FileOptions,
    object::*,
    primitive::Name,
};

macro_rules! pdf_names {
    ($( $name:ident = $val:expr ),* ) => {
   $(
   struct $name;
   impl Into<Primitive> for $name {
     fn into(self) -> Primitive {
       Primitive::Name($val.into())
     }
   }
   )*
   }
}

use clap::Parser;
use image::ImageReader;
use std::io::Cursor;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input PDF file
    #[arg(short, long)]
    input: PathBuf,

    /// Input image file
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
    y: f32,
}
struct Align {
    page_rel: f32,
    page_abs: f32,
    img_rel: f32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let img_data = std::fs::read(&args.image)?;
    let img =
        ImageReader::with_format(Cursor::new(&img_data), image::ImageFormat::Jpeg).decode()?;
    let image_dict = ImageDict {
        width: img.width(),
        height: img.height(),
        color_space: Some(ColorSpace::DeviceRGB),
        bits_per_component: Some(8),
        ..Default::default()
    };
    let image = Stream::new_with_filters(
        image_dict,
        img_data,
        vec![StreamFilter::DCTDecode(DCTDecodeParams {
            color_transform: None,
        })],
    );

    let mut file = FileOptions::cached().open(&args.input).unwrap();
    let page = file.get_page(args.page).expect("no such page");

    let resources = page.resources()?;
    let mut resources2: Resources = (**resources).clone();

    let image_obj = XObject::Image(ImageXObject { inner: image });
    let image_ref = file.create(image_obj)?;

    // assume that name did not exist
    let image_name = Name::from("MyImage");
    resources2
        .xobjects
        .insert(image_name.clone(), image_ref.get_ref());

    let mut ops = page
        .contents
        .as_ref()
        .unwrap()
        .operations(&file.resolver())?;

    let mm = 72.0 / 25.4; // one millimeter
                          // bottom right corner of the page, but 5mm margin
    let h_align = Align {
        img_rel: -1.0,       // move left by image width
        page_rel: 1.0,       // move right by page width
        page_abs: -5.0 * mm, // 5,mm from the right edge
    };
    let v_align = Align {
        img_rel: 0.0,
        page_rel: 0.0,
        page_abs: 5.0 * mm,
    };
    let dpi = 300.;

    let px_scale = 72. / dpi;
    let media_box = page.media_box.unwrap();
    let scale = Point {
        x: img.width() as f32 * px_scale,
        y: img.height() as f32 * px_scale,
    };
    let skew = Point { x: 0.0, y: 0.0 };
    let page_size = Point {
        x: media_box.right - media_box.left,
        y: media_box.top - media_box.bottom,
    };
    let page_origin = Point {
        x: media_box.left,
        y: media_box.bottom,
    };

    let position = Point {
        x: page_origin.x
            + h_align.page_abs
            + h_align.img_rel * scale.x
            + h_align.page_rel * page_size.x,
        y: page_origin.y
            + v_align.page_abs
            + v_align.img_rel * scale.y
            + v_align.page_rel * page_size.y,
    };

    ops.append(&mut vec![
        Op::Save, // ADD IMAGE START
        Op::Transform {
            matrix: Matrix {
                // IMAGE MANIPULATION
                a: scale.x,
                d: scale.y,
                b: skew.x,
                c: skew.y,
                e: position.x,
                f: position.y,
            },
        },
        Op::XObject { name: image_name }, // IMAGE
        Op::Restore,                      // ADD IMAGE STOP
    ]);

    let mut page2: Page = (*page).clone();
    page2.contents = Some(Content::from_ops(ops));
    page2.resources = Some(file.create(resources2)?.into());

    PageRc::update(page2, &page, &mut file)?;

    file.save_to(&args.output)?;

    Ok(())
}
