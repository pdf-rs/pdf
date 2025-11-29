use pdf::{PdfError, object::{GenNr, NoResolve, ObjNr, Object}, parser::{Lexer, ParseFlags, parse_indirect_object}, primitive::{Name, PdfString, Primitive}};
use pdf_derive::Object;

fn main() {
    let file = std::env::args().nth(1).unwrap();
    let data = std::fs::read(&file).unwrap();

    let mut lexer = Lexer::new(&data);

    while let Ok((re, prim)) = parse_indirect_object(&mut lexer, &NoResolve, None, ParseFlags::ANY) {
        dbg!(&re, &prim);
        let root = Root::from_primitive(prim, &NoResolve).unwrap();
        for f in root.fdf.fields {
            if let Some(val) = f.value {
                let s = val.to_string_lossy();
                for line in s.split(['\r', '\n']) {
                    println!("{line}");
                }
            }
        }
    }
}


#[derive(Object)]
struct Root {
    #[pdf(key="FDF")]
    fdf: Fdf
}

#[derive(Object)]
struct Fdf {
    #[pdf(key="Fields")]
   fields: Vec<Field>
}

#[derive(Object)]
struct Field {
    #[pdf(key = "T")]
    key: PdfString,

    #[pdf(key = "V")]
    value: Option<PdfString>,
}
