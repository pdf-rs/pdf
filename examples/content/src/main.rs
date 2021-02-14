extern crate pdf;

use std::env;
use std::path::PathBuf;

use pdf::file::File;
use pdf::object::*;
use pdf::error::PdfError;
use pdf::content::*;
use pdf::build::*;

macro_rules! ops {
    ($($name:ident $($arg:expr),* ;)*) => (
        vec![ $(Operation::new(stringify!($name), vec![$($arg.into()),*]) ),* ]
    );
}


fn main() -> Result<(), PdfError> {
    let path = PathBuf::from(env::args_os().nth(1).expect("no file given"));
    
    let mut file = File::<Vec<u8>>::open(&path).unwrap();

    let mut pages = Vec::new();
    for page in file.pages().take(1) {
        let page = page.unwrap();
        if let Some(ref c) = page.contents {
            println!("{}", c);
        }

        let content = Content {
            operations: ops![
                m 100, 100;
                l 100, 200;
                l 200, 100;
                l 200, 200;
                S;
            ]
        };
        pages.push(PageBuilder::from_content(content));
    }
    let catalog = CatalogBuilder::from_pages(pages)
        .build(&mut file).unwrap();
    
    file.update_catalog(catalog);

    file.save_to(path.join(".modified.pdf"))?;

    Ok(())
}
