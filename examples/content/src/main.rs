extern crate pdf;

use std::env::args;

use pdf::file::File;
use pdf::object::*;
use pdf::error::PdfError;
use pdf::content::*;

macro_rules! ops {
    ($($name:ident $($arg:expr),* ;)*) => (
        vec![ $(Operation::new(stringify!($name), vec![$($arg.into()),*]) ),* ]
    );
}


fn main() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    
    let file = File::<Vec<u8>>::open(&path).unwrap();
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
        let page2 = Page {
            contents: Some(content),
            media_box: Some(Rect {
                top: 0.,
                left: 0.,
                bottom: 500.,
                right: 500.
            }),
            crop_box: None,
            trim_box: None,
            parent: page.parent,
            resources: None,
        };

        let mut pages = PagesNode::to_tree(&file.get_root().pages);
        pages.add_page(page2);
    }

    Ok(())
}
