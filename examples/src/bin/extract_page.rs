use std::path::PathBuf;

use pdf::{
    error::PdfError,
    file::FileOptions,
    object::*,
    build::*,
    primitive::{PdfString, Name}, content::{Op, Color, Cmyk, Matrix}, font::{Font, TFont, FontData},
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

    let mut new_page = PageBuilder::clone_page(&old_page, &mut importer)?;
    importer.finish().verify(&builder.storage.resolver())?;

    let font = Font {
        data: FontData::TrueType(TFont{
            base_font: Some(Name::from("Helvetica")),
            first_char: None,
            font_descriptor: None,
            last_char: None,
            widths: None,
        }),
        encoding: Some(pdf::encoding::Encoding::standard()),
        name: None,
        subtype: pdf::font::FontType::TrueType,
        to_unicode: None,
        _other: Default::default()
    };
    let font_name = Name::from("F42");
    new_page.resources.fonts.insert(font_name.clone(), builder.storage.create(font)?.into());

    new_page.ops.push(Op::BeginText);
    let label = format!("{} page {}", args.input.file_name().unwrap().to_string_lossy(), args.page).into_bytes();
    let mut text_ops = vec![
        Op::FillColor { color: Color::Cmyk(Cmyk { cyan: 0.0, magenta: 0.0, key: 1.0, yellow: 0.0})},
        Op::BeginText,
        Op::SetTextMatrix { matrix: Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1., e: 10., f: 10. }},
        Op::TextFont { name: font_name.clone(), size: 20. },
        Op::TextDraw { text: PdfString::new(label.into()) },
        Op::EndText
    ];
    new_page.ops.append(&mut text_ops);

    pages.push(new_page);
    
    let catalog = CatalogBuilder::from_pages(pages);
    
    let mut info = InfoDict::default();
    info.title = Some(PdfString::from("test"));
    
    let data = builder.info(info).build(catalog)?;

    std::fs::write(&args.output, data)?;

    Ok(())
}
