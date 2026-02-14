use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Parser, Subcommand, ValueEnum};
use pdf::content::{Color, FormXObject, Matrix, Op, Point, Rect, Rgb, ViewRect, Winding, serialize_ops};
use pdf::enc::StreamFilter;
use pdf::error::PdfError;
use pdf::file::FileOptions;

use pdf::font::Font;
use pdf::object::*;
use pdf::primitive::{Dictionary, Name, PdfStream, PdfString, Primitive};
use uuid::Uuid;

#[derive(Parser)]
struct ProgArgs {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    #[command(flatten)]
    trigger: Trigger,

    #[command(flatten)]
    action: CommandAction,
}

#[derive(Args, Clone)]
#[group(required = false, multiple = false)]
struct Trigger {
    #[arg(long)]
    open: bool,

    #[arg(long)]
    page_view: Option<u32>,
}

#[derive(Args, Clone)]
#[group(required = false, multiple = false)]
struct CommandAction {
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    js: Option<PathBuf>,
}

fn make_js_action(updater: &mut impl Updater, js_path: &Path) -> Result<Action, PdfError> {
    let mut action = Action::new(ActionType::JavaScript);
    println!("{js_path:?}");
    let data = encode_text(&std::fs::read_to_string(js_path).unwrap());
    let stream = Stream::new_compressed((), &data, StreamFilter::FlateDecode(Default::default()))?;
    let stream = updater.create(stream)?;
    action.js = Some(StringOrStream::Stream(stream.into()));

    Ok(action)
}

fn main() -> Result<(), PdfError> {
    let args = ProgArgs::parse();

    let mut file = FileOptions::cached().open(&args.input)?;

    let page_ref = file.get_page(0).unwrap();
    let mut page = (*page_ref).clone();
    let mut page_annots = page.annotations.load(&file.resolver())?.owned();

    let mut catalog = file.get_root().clone();

    let font = Font::standard("Helvetica");
    let font_name = Name::from("Helvetica");
    let font = file.create(font)?;
    let mut fonts = HashMap::new();
    fonts.insert(font_name.clone(), font.into());
    let resources = Resources {
        fonts,
        ..Default::default()
    };
    let resources = file.create(resources)?;

    let da_ops = vec![
        Op::TextFont {
            name: font_name.clone(),
            size: 8.0,
        },
        Op::FillColor { color: Color::Rgb(Rgb { red: 0.1, green: 0.0, blue: 0.0 })  }
    ];
    let button_ops = vec![
        Op::FillColor { color: Color::Rgb(Rgb { red: 0.1, green: 1.0, blue: 1.0 })  },
        Op::Rect { rect: pdf::content::ViewRect {x: 0.0, y: 0.0, width: 100., height: 40. } },
        Op::Fill { winding: pdf::content::Winding::NonZero },
        Op::BeginText,
        Op::TextFont {
            name: font_name,
            size: 12.0,
        },
        Op::FillColor { color: Color::Rgb(Rgb { red: 0.5, green: 0.0, blue: 0.0 })  },
        Op::MoveTextPosition { translation: Point { x: 20., y: 15. } },
        Op::TextDraw { text: "Hello".into() },
        Op::EndText,
        Op::Restore
    ];

    {
        // Debug field
        let field_promise = file.promise();
        let mut annot = Annot::new("Widget".into());
        annot.rect = Some(Rectangle { left: 20., bottom: 20., right: 200., top: 220. });
        annot.parent = Some(field_promise.get_inner());
        let annot = file.create(annot)?;

        let mut field = FieldDictionary::new(FieldType::Text);
        field.name = Some("Debug".into());
        field.default_appearance = Some(serialize_ops(&da_ops).unwrap().into());
        field.flags = (TextFieldFlags::MULTILINE | TextFieldFlags::READ_ONLY).bits();
        field.value = Primitive::String("Test".into());
        field.kids.push(Merged::merged_ref_b(&annot));
        field.default_resources = Some(resources.clone().into());

        let text_field = file.fulfill(field_promise, field)?;
        catalog.forms.get_or_insert_default().fields.push(text_field);

        page_annots.push(annot.into());
    }

    let id_a = Uuid::new_v4().to_string();
    let id_b = Uuid::new_v4().to_string();

    if true {
        // button
        let field_promise = file.promise();
        let mut annot = Annot::new("Widget".into());
        let mut field = FieldDictionary::new(FieldType::Button);

        // AP
        let xof = FormDict {
            bbox: Rectangle { left: 0., bottom: 0., right: 100., top: 40. },
            matrix: Some(Matrix::default()),
            form_type: 1,
            resources: Some(resources.clone().into()),

            .. Default::default()
        };
        let xo = file.create(FormXObject {
            stream: Stream::new(xof, &serialize_ops(&button_ops).unwrap()).unwrap()
        })?;

        let button_as = AppearanceStreams {
            normal: file.create(AppearanceStreamEntry::Single(xo.into()))?.into(),
            down: None,
            rollover: None
        };
        annot.appearance_streams = Some(file.create(button_as)?.into());

        // DA
        field.default_appearance = Some(serialize_ops(&da_ops).unwrap().into());

        // F
        annot.annot_flags = 4;

        // Ff
        field.flags = (ButtonFieldFlags::PUSHBUTTON).bits();

        // H
        annot.highlighting_mode = Some(HighlightingMode::None);

        // MK
        annot.appearance_characteristics = Some(AppearanceCharacteristic {
            // BC
            border_color: Some(Color::Rgb(Rgb { red: 0.0, green: 0.5, blue: 0.0 })),

            // BG
            background_color: Some(Color::Rgb(Rgb { red: 1.0, green: 1.0, blue: 0.0 })),

            // CA
            caption: Some("Button".into()),
            ..Default::default()
        }.into());

        // P
        annot.parent = Some(field_promise.get_inner());

        // Rect
        annot.rect = Some(Rectangle { left: 100., bottom: 350., right: 320., top: 420. });
        //annot.other.insert("T", Name("Test".into()));

        field.name = Some("B2".into());

        annot.action = Some(make_js_action(&mut file, Path::new("mousedown.js")).unwrap().into());

        let annot = file.create(annot)?;
        field.kids.push(Merged::merged_ref_b(&annot));

        let button_field = file.fulfill(field_promise, field)?;
        catalog.forms.get_or_insert_default().fields.push(button_field);
        page_annots.push(annot.into());
    }

    // QR code
    {
        let url = format!("https://cypress.webredirect.org/verify/{id_a}/{id_b}");
        let fxo = qr_code(&url, &mut file)?;

        // button
        let field_promise = file.promise();
        let mut annot = Annot::new("Widget".into());
        let mut field = FieldDictionary::new(FieldType::Button);

        // AP
        annot.appearance_streams = Some(file.create(AppearanceStreams {
            normal: AppearanceStreamEntry::Single(fxo.clone().into()).into(),
            down: None,
            rollover: None
        })?.into());

        // DA
        field.default_appearance = Some(serialize_ops(&da_ops).unwrap().into());

        // F
        annot.annot_flags = 4;

        // MK
        annot.appearance_characteristics = Some(AppearanceCharacteristic {
            // BG
            background_color: Some(Color::Rgb(Rgb { red: 1.0, green: 1.0, blue: 0.0 })),

            // CA
            caption: Some("Button".into()),

            // I
            icon: Some(fxo.get_ref()),

            // TP
            text_position: Some(1),

            ..Default::default()
        }.into());

        // Ff
        field.flags = (ButtonFieldFlags::PUSHBUTTON).bits();

        // H
        annot.highlighting_mode = Some(HighlightingMode::None);

        // P
        annot.parent = Some(field_promise.get_inner());

        // Rect
        annot.rect = Some(Rectangle { left: 400., bottom: 300., right: 500., top: 400. });

        let mut link_action = Action::new(ActionType::URI);
        link_action.uri = Some(url.into());
        annot.action = Some(link_action.into());

        field.name = Some("QR".into());

        let annot = file.create(annot)?;
        field.kids.push(Merged::merged_ref_b(&annot));

        let button_field = file.fulfill(field_promise, field)?;
        catalog.forms.get_or_insert_default().fields.push(button_field);
        page_annots.push(annot.into());
    }


    page.annotations = Lazy::new(page_annots, &mut file)?;

    PageRc::update(page, &page_ref, &mut file).unwrap();

    let js_names: Vec<(PdfString, MaybeRef<Action>)> = vec![
        ("Functions".into(), make_js_action(&mut file, Path::new("functions.js")).unwrap().into()),
        ("Init".into(), make_js_action(&mut file, Path::new("init.js")).unwrap().into())
    ];
    let js_names = NameTree::build_flat(js_names);
    let mut names = NameDictionary::default();
    names.javascript = Some(js_names.into());
    catalog.names = Some(file.create(names).unwrap().into());

    let action;
    if let Some(uri) = args.action.url {
        let mut a  = Action::new(ActionType::URI);
        a.uri = Some((&*uri).into());
        action = Some(a);
    } else if let Some(js_path) = args.action.js {
        action = Some(make_js_action(&mut file, &js_path).unwrap());
    } else {
        action = None;
    }

    if let Some(action) = action {
        if args.trigger.open == true {
            catalog.open_action = Some(Either::Left(action));
        } else if let Some(page_nr) = args.trigger.page_view {
            let page_ref = file.get_page(page_nr).unwrap();
            let mut page = (*page_ref).clone();
            page.aa.get_or_insert_default().open = Some(action);

            PageRc::update(page, &page_ref, &mut file).unwrap();
        }
    }

    file.trailer.id = vec![
        id_a.into(),
        id_b.into()
    ];
    file.update_catalog(catalog)?;
    file.save_to(&args.output)?;

    Ok(())
}

fn qr_code(url: &str, updater: &mut impl Updater) -> Result<RcRef<FormXObject>, PdfError> {
    let qr = qrcodegen::QrCode::encode_text(&url, qrcodegen::QrCodeEcc::Low).unwrap();
    let s = qr.size();
    let stride = (s + 7) / 8;
    let mut data = vec![0; s as usize * stride as usize];
    for y in 0..s {
        for x in 0..s {
            let bit = qr.get_module(x, y);
            let row_idx = (y * stride) as usize;
            data[row_idx + x as usize / 8] |= (bit as u8) << (x % 8);
        }
    }
    let i = ImageDict {
        width: s as u32,
        height: s as u32,
        color_space: Some(ColorSpace::DeviceGray),
        bits_per_component: Some(1),
        intent: None,
        image_mask: false,
        mask: None,
        decode: Some(vec![1.0, 0.0]),
        interpolate: false,
        struct_parent: None,
        id: None,
        smask: None,
        other: Default::default()
    };
    let xo = updater.create(XObject::Image(ImageXObject {
        inner: Stream::new_compressed(i, &data, StreamFilter::FlateDecode(Default::default()))?
    }))?;

    let frm_ops = vec![
        Op::Save,
        Op::Transform { matrix: Matrix { a: 100., b: 0.0, c: 0.0, d: 100., e: 0., f: 0. } },
        Op::XObject { name: "QR_im".into() },
        Op::Restore
    ];
    let frm_dit = FormDict {
        bbox: Rectangle { left: 0.0, bottom: 0.0, right: 100., top: 100. },
        form_type: 1,
        matrix: Some(Matrix::default()),
        resources: Some(MaybeRef::Direct(Arc::new(Resources {
            xobjects: [
                ("QR_im".into(), xo.get_ref())
            ].into_iter().collect(),
            .. Default::default()
        }))),
        ..Default::default()
    };
    let fxo = updater.create(FormXObject {
        stream: Stream::new_compressed(frm_dit.clone(), &serialize_ops(&frm_ops)?, StreamFilter::FlateDecode(Default::default()))?
    })?;

    Ok(fxo)
}
