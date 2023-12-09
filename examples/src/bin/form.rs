extern crate pdf;

use std::collections::HashMap;
use std::env::args;

use pdf::content::{FormXObject, Op, serialize_ops};
use pdf::error::PdfError;
use pdf::file::{FileOptions, Log};
use pdf::font::{Font, FontData, TFont};
use pdf::object::*;
use pdf::primitive::{PdfString, Primitive, Name};

fn main() -> Result<(), PdfError> {
    let path = args().nth(1).expect("no file given");
    println!("read: {}", path);

    let mut file = FileOptions::cached().open(&path).unwrap();
    let mut to_update_field: Option<_> = None;

    let page0 = file.get_page(0).unwrap();
    let ops = page0
        .contents.as_ref().unwrap()
        .operations(&file.resolver()).unwrap();

    for annot in &page0.annotations {
        if let Some(ref a) = annot.appearance_streams {
            let normal = file.resolver().get(a.normal);
            if let Ok(normal) = normal {
                match *normal {
                    AppearanceStreamEntry::Single(ref s) => {
                        //dbg!(&s.stream.resources);
                        
                        let mut ops = s.operations(&file.resolver())?;
                        for op in ops.iter_mut() {
                            match op {
                                Op::TextDraw { text } => {
                                    println!("{}", text.to_string_lossy());
                                    *text = PdfString::from("helloo");
                                }
                                _ => {}
                            }
                        }
                        let stream = Stream::new(s.stream.info.info.clone(), serialize_ops(&ops)?);

                        let normal2 = AppearanceStreamEntry::Single(FormXObject { stream });
                        file.update(a.normal.get_inner(), normal2)?;
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(ref forms) = file.get_root().forms {
        println!("Forms:");
        for field in forms.fields.iter().take(1) {
            print!("  {:?} = ", field.name);
            match field.value {
                Primitive::String(ref s) => println!("{}", s.to_string_lossy()),
                Primitive::Integer(i) => println!("{}", i),
                Primitive::Name(ref s) => println!("{}", s),
                ref p => println!("{:?}", p),
            }

            if to_update_field.is_none() {
                to_update_field = Some(field.clone());
            }
        }
    }

    /*
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
    let font_name = Name::from("Helvetica");
    let font = file.create(font)?;
    let mut fonts = HashMap::new();
    fonts.insert("Helvetica".into(), font.into());
    let resources = Resources {
        fonts,
        .. Default::default()
    };
    let resources = file.create(resources)?;
    */

    if let Some(to_update_field) = to_update_field {
        println!("\nUpdating field:");
        println!("{:?}\n", to_update_field);

        let text = "helloo";
        let new_value: PdfString = PdfString::new(text.into());
        let mut updated_field = (*to_update_field).clone();
        updated_field.value = Primitive::String(new_value);

        /*
        let mut form_dict = FormDict {
            bbox: updated_field.rect,
            resources: Some(resources.into()),
            .. Default::default()
        };
        let content = format!("q BT /Helvetica 14 Tf ({text}) ET Q");
        
        let form = FormXObject {
            stream: Stream::new(form_dict, content.into_bytes())
        };
        //dbg!(&form);
        let normal = AppearanceStreamEntry::Single(form);
        let apperance = AppearanceStreams {
            normal: file.create(normal)?.into(),
            down: None,
            rollover: None
        };
        //updated_field.appearance_streams = Some(apperance.into());
        //updated_field.appearance_state = Some("N".into());
        //dbg!(&updated_field);
        */
        let reference = file.update(
            to_update_field.get_ref().get_inner(),
            updated_field,
        )?;

        file.save_to("output/out.pdf")?;

        println!("\nUpdated field:");
        //println!("{:?}\n", reference);
    }

    Ok(())
}
