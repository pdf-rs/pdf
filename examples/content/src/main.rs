use std::env;
use std::path::PathBuf;

use pdf::file::File;
use pdf::error::PdfError;
use pdf::content::*;
use pdf::build::*;

fn main() -> Result<(), PdfError> {
    let path = PathBuf::from(env::args_os().nth(1).expect("no file given"));
    
    let mut file = File::<Vec<u8>>::open(&path).unwrap();

    let mut pages = Vec::new();
    for page in file.pages().take(1) {
        let page = page.unwrap();
        if let Some(ref c) = page.contents {
            println!("{:?}", c);
        }

        let content = Content::from_ops(vec![
            Op::MoveTo { p: Point { x: 100., y: 100. } },
            Op::LineTo { p: Point { x: 100., y: 200. } },
            Op::LineTo { p: Point { x: 200., y: 100. } },
            Op::LineTo { p: Point { x: 200., y: 200. } },
            Op::Close,
            Op::Stroke,
        ]);
        pages.push(PageBuilder::from_content(content));
    }
    let catalog = CatalogBuilder::from_pages(pages)
        .build(&mut file).unwrap();
    
    file.update_catalog(catalog)?;

    file.save_to(path.with_extension("modified.pdf"))?;

    Ok(())
}
