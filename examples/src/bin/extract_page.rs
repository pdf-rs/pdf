use std::path::PathBuf;

use pdf::{
    error::PdfError,
    file::FileOptions,
    object::*,
    build::*,
    primitive::PdfString,
};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file
    #[arg(short, long)]
    input: PathBuf,

    /// Page number
    #[arg(short, long, default_value_t = 0)]
    page: u32,

    /// Output file
    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> Result<(), PdfError> {
    let args = Args::parse();
    
    let old_file = FileOptions::cached().open(&args.input).unwrap();
    let old_page = old_file.get_page(args.page).expect("no such page");
    
    let mut builder = PdfBuilder::new(FileOptions::cached());

    let mut importer = Importer::new(old_file.resolver(), &mut builder.storage);
    let mut pages = Vec::new();

    let new_page = PageBuilder::clone_page(&old_page, &mut importer)?;
    importer.finish().verify(&builder.storage.resolver())?;

    pages.push(new_page);
    
    let catalog = CatalogBuilder::from_pages(pages);
    
    let mut info = InfoDict::default();
    info.title = Some(PdfString::from("test"));
    
    let data = builder.info(info).build(catalog)?;

    std::fs::write(&args.output, data)?;

    Ok(())
}
