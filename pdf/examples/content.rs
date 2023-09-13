use std::env;
use std::path::PathBuf;


use pdf::error::PdfError;
use pdf::content::*;
use pdf::file::FileOptions;




use pdf::object::*;
use pdf::build::*;

use pdf::primitive::PdfString;

fn main() -> Result<(), PdfError> {
    let path = PathBuf::from(env::args_os().nth(1).expect("no file given"));
    
    let mut builder = PdfBuilder::new(FileOptions::cached());

    let mut pages = Vec::new();

    let content = Content::from_ops(vec![
        Op::MoveTo { p: Point { x: 100., y: 100. } },
        Op::LineTo { p: Point { x: 100., y: 200. } },
        Op::LineTo { p: Point { x: 200., y: 200. } },
        Op::LineTo { p: Point { x: 200., y: 100. } },
        Op::Close,
        Op::Stroke,
    ]);
    let mut new_page = PageBuilder::from_content(content);
    new_page.media_box = Some(pdf::object::Rect {
        left: 0.0,
        top: 0.0,
        bottom: 400.0,
        right: 400.0
    });
    let resources = Resources::default();

    /*
    let font = Font {
        name: Some("Test".into()),
        subtype: pdf::font::FontType::TrueType,
        data: FontData::TrueType(TFont {
            base_font: None,
            
        })
    }
    resources.fonts.insert("f1", font);
    */

    new_page.resources = Some(MaybeRef::Indirect(builder.storage.create(resources)?));
    pages.push(new_page);
    
    let catalog = CatalogBuilder::from_pages(pages);
    
    let mut info = InfoDict::default();
    info.title = Some(PdfString::from("test"));
    
    let data = builder.info(info).build(catalog)?;

    std::fs::write(path, data)?;

    Ok(())
}
