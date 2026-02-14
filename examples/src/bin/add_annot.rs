extern crate pdf;

use std::env::args;

use pdf::error::PdfError;
use pdf::file::FileOptions;

use pdf::object::*;
use pdf::primitive::{Dictionary, Name, PdfString, Primitive};

fn run() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);

    let mut old_file = FileOptions::cached().open(&path)?;
    let old_page: PageRc = old_file.get_page(0).unwrap();

    let old_annots = old_page
        .annotations
        .load(&old_file.resolver())
        .expect("can't load annotations");
    let mut annots: Vec<MaybeRef<Annot>> = (*old_annots).clone();
    // let mut new_annots = annots.deref().clone();
    // for annot in &new_annots {
    // dbg!(&annot.subtype);
    // dbg!(&annot.rect);
    // dbg!(&annot.color);
    // dbg!(&annot.transparency);
    // dbg!(&annot.ink_list);
    // dbg!(&annot.line);
    // dbg!(&annot.creation_date);
    // dbg!(&annot.uuid);
    // dbg!(&annot.border_style);
    // dbg!(&annot.popup);
    // dbg!(&annot.other);
    // }

    let mut bs = Dictionary::new();
    bs.insert(Name::from("S"), PdfString::from("/S"));
    bs.insert(Name::from("W"), PdfString::from("3"));
    let mut new_annot = Annot::new(Name::from("Line"));
    new_annot.rect = Some(Rectangle {
        left: 10.,
        bottom: 10.,
        right: 200.,
        top: 200.,
    });
    new_annot.page = Some(old_page.clone());
    new_annot.annot_flags = 4;
    new_annot.color = Some(Primitive::Array(vec![
        Primitive::Integer(1),
        Primitive::Integer(0),
        Primitive::Integer(0),
    ]));
    new_annot.line = Some(vec![10., 100., 20., 200.]);

    let annot_ref = old_file.create(new_annot)?;
    annots.push(MaybeRef::Indirect(annot_ref));

    match old_annots {
        MaybeRef::Direct(_) => {
            // need to update the whole page
            let mut new_page: Page = (*old_page).clone();

            let lazy_annots: Lazy<Vec<MaybeRef<Annot>>> = Lazy::safe(
                MaybeRef::Indirect(old_file.create(annots).unwrap()),
                &mut old_file
            ).unwrap();
            new_page.annotations = lazy_annots;
            PageRc::update(new_page, &old_page, &mut old_file).unwrap();
        }
        MaybeRef::Indirect(r) => {
            // can just update the annot reference
            old_file.update_ref(&r, annots).unwrap();
        }
    }
    old_file.save_to("out.pdf")?;

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        println!("{e}");
    }
}
