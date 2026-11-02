//! Test cases for the PDF 32000-1:2008 mismatches documented in the `*.rs.md`
//! notes beside the sources. One test per finding, named `cNN_*` after the
//! numbering used in `/home/hematite/report/pdf-rs-vs-pdf32000-2026-11-01.md`.
//!
//! Wave 1: syntax layer (7.2 whitespace/comments, 7.3 strings/names/dicts,
//! 7.9.4 dates).
//!
//! The unit cases state the spec behaviour, not the current behaviour, so they
//! FAIL on unpatched master (that is the point: they are the proof of the
//! mismatch) and PASS once the corresponding fix is applied.
//!
//! `fixtures_*` tests write complete PDFs to `PDF_SPEC_CASES_DIR`
//! (default `/home/hematite/report/pdf-cases`) so a second reader
//! (PyMuPDF, via `pdf_to_markdown`) can be run on the very same bytes, then
//! read them back with pdf-rs and check the *result*, not just that nothing
//! errored.

use std::fs;

use pdf::object::NoResolve;
use pdf::parser::{parse, ParseFlags};
use pdf::primitive::{ParsedDate, Primitive, TimeRel};

fn parse_any(data: &[u8]) -> Primitive {
    parse(data, &NoResolve, ParseFlags::ANY).expect("parse failed")
}

fn cases_dir() -> std::path::PathBuf {
    let dir = std::env::var("PDF_SPEC_CASES_DIR")
        .unwrap_or_else(|_| "/home/hematite/report/pdf-cases".to_string());
    let dir = std::path::PathBuf::from(dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------- 7.2.2 whitespace

#[test]
fn c01_form_feed_separates_tokens() {
    // PDF 32000-1:2008 7.2.2 Table 1: FF (0x0C) is a white-space character.
    let p = parse_any(b"[1\x0c2]");
    let arr = p.as_array().expect("array");
    assert_eq!(arr.len(), 2, "FF must separate two tokens, got {:?}", p);
    assert_eq!(arr[0].as_integer().unwrap(), 1);
    assert_eq!(arr[1].as_integer().unwrap(), 2);
}

// ---------------------------------------------------------------- 7.2.4 comments

#[test]
fn c02_comment_ends_at_cr() {
    // 7.2.4: a comment ends at an end-of-line marker, i.e. CR, LF or CRLF.
    let p = parse_any(b"[1 %comment\r2]");
    let arr = p.as_array().expect("array");
    assert_eq!(arr.len(), 2, "comment must end at CR, got {:?}", p);
    assert_eq!(arr[1].as_integer().unwrap(), 2);
}

#[test]
fn c02b_comment_ends_at_crlf() {
    let p = parse_any(b"[1 %comment\r\n2]");
    let arr = p.as_array().expect("array");
    assert_eq!(arr.len(), 2, "comment must end at CRLF, got {:?}", p);
    assert_eq!(arr[1].as_integer().unwrap(), 2);
}

// ---------------------------------------------------------------- 7.3.4.2 strings

#[test]
fn c03_unknown_escape_drops_backslash() {
    // 7.3.4.2: if the character after REVERSE SOLIDUS is not one of Table 3,
    // the REVERSE SOLIDUS shall be ignored.
    let p = parse_any(b"(\\d)");
    let s = p.as_string().unwrap();
    assert_eq!(&s.data[..], b"d", "\\d must read as 'd', got {:?}", &s.data[..]);
}

#[test]
fn c03b_unknown_escape_inside_string() {
    let p = parse_any(b"(a\\zb)");
    let s = p.as_string().unwrap();
    assert_eq!(&s.data[..], b"azb", "got {:?}", &s.data[..]);
}

#[test]
fn c04_eol_inside_string_becomes_lf() {
    // 7.3.4.2: an end-of-line marker inside a literal string is read as 0Ah.
    for input in [&b"(a\rb)"[..], &b"(a\r\nb)"[..], &b"(a\nb)"[..]] {
        let p = parse_any(input);
        let s = p.as_string().unwrap();
        assert_eq!(
            &s.data[..],
            b"a\nb",
            "input {:?}: EOL in a string must read as a single LF",
            input
        );
    }
}

// ---------------------------------------------------------------- 7.3.5 names

#[test]
fn c05_dict_key_is_hex_decoded() {
    // 7.3.5 applies to every name, dictionary keys included.
    let p = parse_any(b"<</A#20B 1>>");
    let dict = p.as_dict().unwrap();
    assert!(
        dict.contains_key("A B"),
        "key /A#20B must decode to 'A B', keys are {:?}",
        dict.keys().collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------- 7.3.9 null entries

#[test]
fn c06_null_dict_value_counts_as_absent() {
    // 7.3.9: an entry whose value is null shall be treated as if absent.
    let p = parse_any(b"<</A null /B 1>>");
    let dict = p.as_dict().unwrap();
    assert_eq!(dict.len(), 1, "null entry must be dropped, got {:?}", dict);
    assert!(!dict.contains_key("A"));
}

// ---------------------------------------------------------------- 7.9.4 dates

#[test]
fn c07_date_serialisation_has_both_apostrophes() {
    // 7.9.4 Table 23: D:YYYYMMDDHHmmSSOHH'mm'
    let d = ParsedDate {
        year: 2026,
        month: 11,
        day: 1,
        hour: 12,
        minute: 34,
        second: 56,
        rel: TimeRel::Later,
        tz_hour: 1,
        tz_minute: 0,
    };
    let date = d.to_date().unwrap();
    assert_eq!(&date.string.data[..], b"D:20261101123456+01'00'");
}

#[test]
fn c07b_date_rejects_impossible_fields() {
    let d = ParsedDate {
        year: 2026,
        month: 13,
        day: 40,
        hour: 12,
        minute: 0,
        second: 0,
        rel: TimeRel::Universal,
        tz_hour: 0,
        tz_minute: 0,
    };
    assert!(d.to_date().is_err(), "month 13/day 40 is not a date");
}

// ---------------------------------------------------------------- full-file fixtures

/// Minimal one-page document with the given content stream.
/// `font_key` is the name used in `/Resources /Font`, the content stream must
/// refer to the same name.
fn text_doc(content: &[u8], font_key: &str) -> Vec<u8> {
    let contents = {
        let mut v = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        v.extend_from_slice(content);
        v.extend_from_slice(b"\nendstream");
        v
    };
    let page = format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] \
         /Resources << /Font << /{} 5 0 R >> >> /Contents 4 0 R >>",
        font_key
    )
    .into_bytes();
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        page,
        contents,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    build_pdf(&objects, 1)
}

fn build_pdf(objects: &[Vec<u8>], root: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n");
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            root,
            xref_pos
        )
        .as_bytes(),
    );
    out
}

/// What pdf-rs made of a whole file: everything drawn by `Tj`, the font
/// resource names referenced by `Tf`, and the keys actually present in
/// `/Resources /Font` of each page.
#[derive(Debug, Default)]
struct Read {
    text: String,
    fonts_used: Vec<String>,
    fonts_available: Vec<String>,
}

fn pdf_rs_read(bytes: &[u8], name: &str) -> Read {
    use pdf::content::Op;

    let file = pdf::file::FileOptions::uncached()
        .load(bytes)
        .unwrap_or_else(|e| panic!("{}: pdf-rs cannot open the file: {}", name, e));
    let resolver = file.resolver();
    let num_pages = file.num_pages();
    assert!(num_pages > 0, "{}: no pages", name);

    let mut out = Read::default();
    for i in 0..num_pages {
        let page = file
            .get_page(i)
            .unwrap_or_else(|e| panic!("{}: page {}: {}", name, i, e));

        let res = page
            .resources()
            .unwrap_or_else(|e| panic!("{}: page {} resources: {}", name, i, e));
        out.fonts_available
            .extend(res.data().fonts.keys().map(|k| k.as_str().to_owned()));

        if let Some(content) = &page.contents {
            let ops = content
                .operations(&resolver)
                .unwrap_or_else(|e| panic!("{}: page {} content: {}", name, i, e));
            for op in ops {
                match op {
                    Op::TextDraw { text } => {
                        out.text.push_str(&String::from_utf8_lossy(&text.data))
                    }
                    Op::TextFont { name, .. } => out.fonts_used.push(name.as_str().to_owned()),
                    _ => {}
                }
            }
        }
    }
    out
}

fn write_case(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = cases_dir().join(format!("{}.pdf", name));
    fs::write(&path, bytes).unwrap();
    path
}

// ---------------------------------------------------------------- wave 2: filters (§7.4)

/// A raw deflate block holding `data` verbatim (stored block, BFINAL=1), so the
/// tests need no compression library.
fn stored_deflate(data: &[u8]) -> Vec<u8> {
    assert!(data.len() <= 0xffff, "stored block too large for one block");
    let len = data.len() as u16;
    let mut out = vec![0x01];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out
}

#[test]
fn c08_asciihex_odd_digit_is_padded() {
    // 7.4.2.1: an odd number of digits: the last digit is followed by 0.
    assert_eq!(pdf::enc::decode_hex(b"f>").unwrap(), vec![0xf0]);
    assert_eq!(pdf::enc::decode_hex(b"61 62 7>").unwrap(), vec![0x61, 0x62, 0x70]);
    // and through the filter interface
    let out = pdf::enc::decode(
        b"f>",
        &pdf::enc::StreamFilter::ASCIIHexDecode,
        &pdf::object::ParseOptions::strict(),
    )
    .unwrap();
    assert_eq!(out, vec![0xf0]);
}

#[test]
fn c09_tiff_predictor_2() {
    // 7.4.4.4: Predictor 2 is TIFF horizontal differencing. Row [10, 20] with
    // Colors 1 is stored as [10, 10].
    use pdf::enc::LZWFlateParams;
    let params = LZWFlateParams {
        predictor: 2,
        n_components: 1,
        bits_per_component: 8,
        columns: 2,
        early_change: 1,
    };
    let data = stored_deflate(&[10, 10]);
    let out = pdf::enc::flate_decode(&data, &params, &pdf::object::ParseOptions::strict()).unwrap();
    assert_eq!(out, vec![10, 20], "TIFF predictor 2 must be un-differenced");
}

#[test]
fn c10_predictor_row_stride_uses_bits_per_component() {
    // 7.4.4.4: a row is ceil(Columns × Colors × BitsPerComponent / 8) bytes.
    // 8 columns × 1 component × 1 bit = 1 byte per row, not 8.
    use pdf::enc::LZWFlateParams;
    let params = LZWFlateParams {
        predictor: 12, // PNG "Up"
        n_components: 1,
        bits_per_component: 1,
        columns: 8,
        early_change: 1,
    };
    // row 1: filter 0, 0x0F; row 2: filter 2 (Up), 0x00 -> 0x0F
    let data = stored_deflate(&[0x00, 0x0f, 0x02, 0x00]);
    let out = pdf::enc::flate_decode(&data, &params, &pdf::object::ParseOptions::strict()).unwrap();
    assert_eq!(out, vec![0x0f, 0x0f], "row stride must follow BitsPerComponent");
}

#[test]
fn c11_crypt_filter_identity_is_transparent() {
    // 7.4.6: /Name /Identity means "no encryption"; the filter must not fail.
    let opts = pdf::object::ParseOptions::strict();
    let filter =
        pdf::enc::StreamFilter::from_kind_and_params("Crypt", Default::default(), &NoResolve)
            .expect("Crypt filter must be constructible");
    let out = pdf::enc::decode(b"hello", &filter, &opts).unwrap();
    assert_eq!(out, b"hello");
}

#[test]
fn c12_ccitt_k0_is_an_error_not_a_panic() {
    // 7.4.5: K = 0 is 1-D modified Huffman; pdf-rs only decodes G4 (K < 0).
    // Conforming already, because the crate's own `unimplemented!` macro
    // (error.rs:365) returns an error instead of panicking — this test keeps it
    // that way rather than documenting a mismatch.
    let params = pdf::enc::CCITTFaxDecodeParams {
        k: 0,
        end_of_line: false,
        encoded_byte_align: false,
        columns: 1728,
        rows: 0,
        end_of_block: true,
        black_is_1: false,
        damaged_rows_before_error: 0,
    };
    let r = pdf::enc::fax_decode(&[0u8; 8], &params);
    assert!(r.is_err(), "K = 0 must yield an error, got {:?}", r);
}

#[test]
fn c13_encode_unsupported_filter_is_an_error() {
    // Writing DCT/CCITT/JPX/JBIG2/RunLength data is not implemented; it must
    // report that instead of panicking (also true on master: `unimplemented!`
    // here is the crate's error-returning macro).
    let r = pdf::enc::encode(b"x", &pdf::enc::StreamFilter::RunLengthDecode);
    assert!(r.is_err(), "RunLengthEncode is not implemented, got {:?}", r);
}

// ---------------------------------------------------------------- wave 3: file structure (§7.5, §7.7)

/// Run `f` on a worker thread and give up after `secs`. A tree walk that never
/// terminates is a bug, but a test must not hang the suite either.
fn with_timeout<T: Send + 'static>(
    secs: u64,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

#[test]
fn c14_page_tree_parent_cycle_terminates() {
    // 7.7.3.2: a page tree is a tree. A crafted file can make /Parent point
    // back into the chain. pdf-rs rejects that while resolving the references
    // ("Recursive reference"), so nothing hangs — this case pins that down, and
    // `inherit()` additionally carries a depth bound as defence in depth.
    let bytes = {
        let objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            // object 2 is its own parent
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 /Parent 2 0 R >>".to_vec(),
            // no /MediaBox and no /Resources: both are inherited
            b"<< /Type /Page /Parent 2 0 R >>".to_vec(),
        ];
        build_pdf(&objects, 1)
    };
    write_case("c14-parent-cycle", &bytes);

    let probe = bytes.clone();
    let result = with_timeout(10, move || {
        match pdf::file::FileOptions::uncached().load(&probe[..]) {
            // rejected at load time
            Err(_) => true,
            // or loaded, but then inheriting must not spin
            Ok(file) => match file.get_page(0) {
                Ok(page) => page.media_box().is_err(),
                Err(_) => true,
            },
        }
    });
    assert_eq!(
        result,
        Some(true),
        "a cyclic /Parent chain must be reported, not loop forever"
    );
}

/// A file whose cross-reference is a stream (7.5.8), with `extra` entries for
/// object numbers after the stream itself: `(type, field1, field2)`.
fn xref_stream_doc(objects: &[Vec<u8>], root: u32, extra: &[(u8, u32, u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n");
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len() as u32);
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_nr = objects.len() as u32 + 1;
    let xref_off = out.len() as u32;

    // /W [1 4 2]: type, field 1, field 2
    let mut data = Vec::new();
    let mut entry = |t: u8, a: u32, b: u16| {
        data.push(t);
        data.extend_from_slice(&a.to_be_bytes());
        data.extend_from_slice(&b.to_be_bytes());
    };
    entry(0, 0, 0xffff);
    for off in &offsets {
        entry(1, *off, 0);
    }
    entry(1, xref_off, 0);
    for &(t, a, b) in extra {
        entry(t, a, b);
    }

    let total = xref_nr + 1 + extra.len() as u32;
    let mut enc = Vec::new();
    for b in &data {
        enc.extend_from_slice(format!("{:02X}", b).as_bytes());
    }
    enc.push(b'>');

    out.extend_from_slice(
        format!(
            "{xref_nr} 0 obj\n<< /Type /XRef /Size {total} /Index [0 {total}] /W [1 4 2] \
             /Root {root} 0 R /Filter /ASCIIHexDecode /Length {} >>\nstream\n",
            enc.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&enc);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    out
}

#[test]
fn c15_reserved_xref_stream_type_is_ignored() {
    // 7.5.8.3: "only types 0, 1, and 2 are allowed. Any other value shall be
    // interpreted as a reference to the null object" — a type 3 entry must not
    // make the whole file unreadable.
    let content = b"BT /F1 24 Tf 72 120 Td (Hello) Tj ET";
    let mut contents = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    contents.extend_from_slice(content);
    contents.extend_from_slice(b"\nendstream");
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_vec(),
        contents,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    // one reserved entry for object 7 (nothing refers to it)
    let bytes = xref_stream_doc(&objects, 1, &[(3, 0, 0)]);
    write_case("c15-reserved-xref-type", &bytes);

    let r = pdf_rs_read(&bytes, "c15-reserved-xref-type");
    assert_eq!(r.text, "Hello", "file with a reserved xref entry type, got {:?}", r);
}

#[test]
fn c16_newer_compressed_xref_entry_wins() {
    // 7.5.6: the most recent copy of each object shall be the one accessed.
    // Sections are merged newest first, so a later (older) section must not
    // replace an entry that is already known to be compressed.
    use pdf::xref::{XRef, XRefSection, XRefTable};

    let mut table = XRefTable::new(4);
    let mut newer = XRefSection::new(1);
    newer.entries.push(XRef::Stream { stream_id: 9, index: 0 });
    table.add_entries_from(newer).unwrap();

    let mut older = XRefSection::new(1);
    older.add_inuse_entry(1234, 0);
    table.add_entries_from(older).unwrap();

    let entry = table
        .entries()
        .find(|(nr, _)| *nr == 1)
        .map(|(_, e)| e.clone())
        .expect("entry for object 1");
    assert!(
        matches!(entry, XRef::Stream { stream_id: 9, index: 0 }),
        "the newer compressed entry must survive, got {:?}",
        entry
    );
}

#[test]
fn fixtures_control_plain() {
    // No anomalies: the file both mupdf and pdf-rs must read (control).
    let bytes = text_doc(b"BT /F1 24 Tf 72 120 Td (Hello) Tj ET", "F1");
    write_case("c00-control", &bytes);
    let r = pdf_rs_read(&bytes, "c00-control");
    assert_eq!(r.text, "Hello", "control file, got {:?}", r);
}

#[test]
fn fixtures_ff_whitespace() {
    // C1 as a real file: tokens separated by FF only.
    let bytes = text_doc(
        b"BT\x0c/F1 24 Tf\x0c72 120 Td\x0c(Hello) Tj\x0cET",
        "F1",
    );
    write_case("c01-ff-whitespace", &bytes);
    let r = pdf_rs_read(&bytes, "c01-ff-whitespace");
    assert_eq!(r.text, "Hello", "FF must separate operators, got {:?}", r);
}

#[test]
fn fixtures_comment_cr() {
    // C2 as a real file: comments terminated by CR, and a comment that contains
    // an unbalanced '(' — legal per 7.2.4, text inside a comment is ignored.
    // pdf-rs does not skip the comment, so its text enters the token stream.
    let bytes = text_doc(
        b"BT /F1 24 Tf 72 120 Td % balance ( note\r(Hello) Tj ET",
        "F1",
    );
    write_case("c02-comment-cr", &bytes);
    let r = pdf_rs_read(&bytes, "c02-comment-cr");
    assert_eq!(r.text, "Hello", "comment text must be ignored, got {:?}", r);
}

#[test]
fn fixtures_unknown_escape() {
    // C3 as a real file: (\d) per 7.3.4.2 reads as "d".
    let bytes = text_doc(b"BT /F1 24 Tf 72 120 Td (\\d) Tj ET", "F1");
    write_case("c03-unknown-escape", &bytes);
    let r = pdf_rs_read(&bytes, "c03-unknown-escape");
    assert_eq!(r.text, "d", "\\d must read as 'd', got {:?}", r);
}

#[test]
fn fixtures_escaped_font_key() {
    // C5 as a real file: the resource key is written /F#31, the content stream
    // refers to /F1. Only a reader that decodes #XX in dictionary keys finds it.
    let bytes = text_doc(b"BT /F1 24 Tf 72 120 Td (Hello) Tj ET", "F#31");
    write_case("c05-escaped-font-key", &bytes);
    let r = pdf_rs_read(&bytes, "c05-escaped-font-key");
    assert_eq!(
        r.fonts_available,
        vec!["F1".to_string()],
        "/F#31 must be stored as /F1, got {:?}",
        r
    );
    assert_eq!(r.text, "Hello", "got {:?}", r);
}
