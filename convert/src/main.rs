use pdf::file::File as PdfFile;
use pdf::object::*;
use pdf::error::PdfError;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use pdf_render::Cache;
use pathfinder_export::{FileFormat, Export};
use pathfinder_geometry::transform2d::Transform2F;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
struct Opt {
    #[structopt(long = "dpi", default_value = "300")]
    dpi: f32,

    /// Format to generate. (svg | png | ps | pdf)
    #[structopt(short = "f", long="format")]
    format: String,

    /// (first) page to generate
    #[structopt(short = "p", long="page", default_value="0")]
    page: u32,

    /// Number of pages to generate, defaults to 1
    #[structopt(short = "n", long="pages", default_value="1")]
    pages: u32,

    #[structopt(long = "placeholder", default_value="\"{}\"")]
    placeholder: String,

    /// Number of digits to zero-pad the page number to
    #[structopt(long = "digits", default_value="1")]
    digits: usize,

    /// Input file
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output file. use '{}' (can be chaged via --palaceholder) as a replacement for the page
    output: String,
}


fn main() -> Result<(), PdfError> {
    env_logger::init();
    let opt = Opt::from_args();

    let format = match opt.format.as_str() {
        "svg" => FileFormat::SVG,
        "pdf" => FileFormat::PDF,
       // "png" => FileFormat::PNG,
        "ps" => FileFormat::PS,
        _ => panic!("invalid format")
    };

    if opt.pages > 1 {
        assert!(opt.output.contains(&opt.placeholder), "output name does not contain a placeholder");
    }

    let transform = Transform2F::from_scale(opt.dpi / 25.4);

    println!("read: {:?}", opt.input);
    let file = PdfFile::<Vec<u8>>::open(&opt.input)?;
    
    let mut cache = Cache::new();
    for (i, page) in file.pages().enumerate().skip(opt.page as usize).take(opt.pages as usize) {
        println!("page {}", i);
        let p: &Page = &*page.unwrap();
        let (scene, _) = cache.render_page(&file, p, transform)?;
        let output = if opt.pages > 1 {
            let replacement = format!("{page:0digits$}", page=i, digits=opt.digits);
            opt.output.replace(opt.placeholder.as_str(), &replacement)
        } else {
            opt.output.clone()
        };
        let mut writer = BufWriter::new(File::create(&output)?);
        scene.export(&mut writer, format)?;
    }
    Ok(())
}
