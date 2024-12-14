extern crate pdf;


use std::env::args;
use std::ops::Deref;

use pdf::content::{FormXObject, Op, serialize_ops};
use pdf::error::PdfError;
use pdf::file::{FileOptions, Log};
use pdf::font::{Font, FontData, TFont};
use pdf::object::*;
use pdf::primitive::{Dictionary, Name, PdfString, Primitive};

fn run() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);

    let mut old_file = FileOptions::cached().open(&path)?;
    let mut old_page: PageRc = old_file.get_page(0).unwrap();
    
    let mut annots = old_page.annotations.load(&old_file.resolver()).expect("can't load annotations");
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
    let new_annot = Annot {
        subtype: Name::from("Line"),
        // rect: Some(Rectangle {
        //     left: 89.774,
        //     bottom: 726.55,
        //     right: 300.961,
        //     top: 742.55,
        // }),
        rect: None,
        contents: None,
        page: Some(old_page.clone()),
        border: None,
        annotation_name: None,
        date: None,
        annot_flags: 4,
        appearance_streams: None,
        appearance_state: None,
        color: Some(Primitive::Array(
            vec![Primitive::Integer(1), Primitive::Integer(0), Primitive::Integer(0)]
            )),
        ink_list: None,
        line: Some(Primitive::Array(
            vec![
                Primitive::Number(95.774), 
                Primitive::Number(734.237), 
                Primitive::Number(320.961),
                Primitive::Number(734.863)
                ]
        )),
        // creation_date: None,
        // uuid: None,
        // border_style: Some(bs),
        // border_style: None,
        // popup: None,
        other: Dictionary::new(),
        // transparency: Some(1.0),
        // transparency: None,
    };

    let annot_ref = old_file.create(new_annot)?;
    annots.push(MaybeRef::Indirect(annot_ref));

    // let lazy_annots = Lazy::from_primitive(
    //     annots.to_primitive(&mut FileOptions::cached().storage()).unwrap(), 
    //     &file.resolver()
    // );

    // old_page.update_annots(annots, &old_file.resolver(), &mut FileOptions::cached().storage());
    // let old_annots = old_page.annotations.to_primitive(&mut old_file).unwrap();


    // let layz_annots = Lazy::from(annots);
    // match annots {
    //     MaybeRef::Indirect(annot) => {
    //         old_page.annotations = Lazy::from(annot);
    //     }
    // }

    old_file.update(old_page.get_plain_ref(), old_page);
    old_file.save_to("/Users/apple/Downloads/test_pdf/out.pdf")?;

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        println!("{e}");
    }
}