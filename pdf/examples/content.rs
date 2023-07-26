use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use pdf::error::PdfError;
use pdf::content::*;
use pdf::file::FileOptions;
use pdf::file::Trailer;
use pdf::object::*;
use pdf::build::*;
use pdf::primitive::Dictionary;
use pdf::primitive::PdfString;

fn main() -> Result<(), PdfError> {
    let path = PathBuf::from(env::args_os().nth(1).expect("no file given"));
    
    let mut storage = FileOptions::cached().storage();

    let mut pages = Vec::new();

    let content = Content::from_ops(vec![
        Op::MoveTo { p: Point { x: 100., y: 100. } },
        Op::LineTo { p: Point { x: 100., y: 200. } },
        Op::LineTo { p: Point { x: 200., y: 100. } },
        Op::LineTo { p: Point { x: 200., y: 200. } },
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
    new_page.resources = Some(MaybeRef::Direct(Arc::new(resources)));
    pages.push(new_page);
    
    let catalog = CatalogBuilder::from_pages(pages)
        .build(&mut storage).unwrap();
    
    let mut info = Dictionary::new();
    info.insert("Title", PdfString::from("test"));
    
    let mut trailer = Trailer {
        root: storage.create(catalog)?,
        encrypt_dict: None,
        size: 0,
        id: vec!["foo".into(), "bar".into()],
        info_dict: Some(info),
        prev_trailer_pos: None,
    };
    let data = storage.save(&mut trailer)?;

    std::fs::write(path, data)?;

    Ok(())
}
