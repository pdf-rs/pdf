use pdf::font::Font;
use pdf::object::{NoResolve, Object};
use pdf::primitive::{Dictionary, Primitive};

fn font_with_w(w: Vec<Primitive>) -> Font {
    let mut desc = Dictionary::new();
    desc.insert("FontName", Primitive::Name("F".into()));

    let mut d = Dictionary::new();
    d.insert("Type", Primitive::Name("Font".into()));
    d.insert("Subtype", Primitive::Name("CIDFontType0".into()));
    d.insert("BaseFont", Primitive::Name("F".into()));
    d.insert("CIDSystemInfo", Primitive::Dictionary(Dictionary::new()));
    d.insert("FontDescriptor", Primitive::Dictionary(desc));
    d.insert("W", Primitive::Array(w));
    Font::from_primitive(Primitive::Dictionary(d), &NoResolve).unwrap()
}

#[test]
fn w_array_empty_underflow() {
    let font = font_with_w(vec![Primitive::Integer(0), Primitive::Array(vec![])]);
    let r = font.widths(&NoResolve);
    assert!(matches!(r, Ok(Some(_))));
}

/// Same expression, reached through the indirect-reference arm.
struct EmptyArrayResolve;
impl pdf::object::Resolve for EmptyArrayResolve {
    fn resolve_flags(
        &self,
        _: pdf::object::PlainRef,
        _: pdf::parser::ParseFlags,
        _: usize,
    ) -> pdf::error::Result<Primitive> {
        Ok(Primitive::Array(vec![]))
    }
    fn get<T: Object + datasize::DataSize>(
        &self,
        _: pdf::object::Ref<T>,
    ) -> pdf::error::Result<pdf::object::RcRef<T>> {
        Err(pdf::error::PdfError::Reference)
    }
    fn options(&self) -> &pdf::object::ParseOptions {
        static STRICT: pdf::object::ParseOptions = pdf::object::ParseOptions::strict();
        &STRICT
    }
    fn get_data_or_decode(
        &self,
        _: pdf::object::PlainRef,
        _: std::ops::Range<usize>,
        _: &[pdf::enc::StreamFilter],
    ) -> pdf::error::Result<std::sync::Arc<[u8]>> {
        Err(pdf::error::PdfError::Reference)
    }
    fn stream_data(
        &self,
        _: pdf::object::PlainRef,
        _: std::ops::Range<usize>,
    ) -> pdf::error::Result<std::sync::Arc<[u8]>> {
        Err(pdf::error::PdfError::Reference)
    }
}

#[test]
fn w_array_empty_underflow_via_reference() {
    let font = font_with_w(vec![
        Primitive::Integer(0),
        Primitive::Reference(pdf::object::PlainRef { id: 1, gen: 0 }),
    ]);
    let r = font.widths(&EmptyArrayResolve);
    assert!(matches!(r, Ok(Some(_))));
}
