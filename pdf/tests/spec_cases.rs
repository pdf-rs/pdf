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
//! `fixtures_*` tests write complete PDFs to `$PDF_SPEC_CASES_DIR` if that is
//! set (`/home/hematite/report/pdf-cases` for the author's own run), else to
//! `$CARGO_TARGET_TMPDIR/pdf-spec-cases`, so a second reader (PyMuPDF, via
//! `pdf_to_markdown`) can be run on the very same bytes, then read them back
//! with pdf-rs and check the *result*, not just that nothing errored.

use std::fs;

use pdf::object::{NoResolve, ParseOptions};
use pdf::parser::{parse, ParseFlags};
use pdf::primitive::{Name, ParsedDate, Primitive, TimeRel};

fn parse_any(data: &[u8]) -> Primitive {
    parse(data, &NoResolve, ParseFlags::ANY).expect("parse failed")
}

fn cases_dir() -> std::path::PathBuf {
    let dir = match std::env::var_os("PDF_SPEC_CASES_DIR") {
        Some(dir) => std::path::PathBuf::from(dir),
        // Cargo's per-target scratch directory: writable everywhere, including
        // CI, and cleaned by `cargo clean`. Never default to a fixed absolute
        // path in $HOME: on a runner that path either does not exist or is not
        // writable, and `create_dir_all` would panic the whole suite.
        None => std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("pdf-spec-cases"),
    };
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
    build_pdf_with_trailer(objects, root, "")
}

/// `extra_trailer` is appended to the trailer dictionary verbatim (used for
/// `/Info`, `/ID` and `/Encrypt`).
fn build_pdf_with_trailer(objects: &[Vec<u8>], root: u32, extra_trailer: &str) -> Vec<u8> {
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
            "trailer\n<< /Size {} /Root {} 0 R{} >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            root,
            extra_trailer,
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
    /// `/Info /Title`, as decrypted by pdf-rs.
    title: Option<String>,
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
    out.title = file
        .trailer
        .info_dict
        .as_ref()
        .and_then(|i| i.title.as_ref())
        .map(|t| String::from_utf8_lossy(&t.data).into_owned());
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

// ---------------------------------------------------------------- wave 4: crypt filters (§7.6)

// 7.6.3.2 Table 21: the 32-byte padding string.
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01,
    0x08, 0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53,
    0x69, 0x7A,
];

/// 7.6.3.3: permissions of the test files (`-4` = all bits set but reserved
/// ones, no password required).
const PERMS: i32 = -4;

fn md5_of(parts: &[&[u8]]) -> [u8; 16] {
    let mut hash = md5::Context::new();
    for p in parts {
        hash.consume(p);
    }
    *hash.compute()
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut d = data.to_vec();
    pdf::crypt::Rc4::encrypt(key, &mut d);
    d
}

fn pad_password(pw: &[u8]) -> [u8; 32] {
    let mut out = PAD;
    let n = pw.len().min(32);
    out[..n].copy_from_slice(&pw[..n]);
    out
}

/// Algorithm 3: the `/O` entry, for RC4 revisions 2..4.
fn compute_o(owner_pw: &[u8], user_pw: &[u8], revision: u32, key_len: usize) -> Vec<u8> {
    let mut ok = md5_of(&[&pad_password(owner_pw)]).to_vec();
    if revision >= 3 {
        for _ in 0..50 {
            ok = md5_of(&[&ok[..key_len]]).to_vec();
        }
    }
    let ok = &ok[..key_len];
    let data = pad_password(user_pw);
    if revision == 2 {
        rc4(ok, &data)
    } else {
        let mut data = rc4(ok, &data);
        for round in 1..=19u8 {
            let key: Vec<u8> = ok.iter().map(|b| b ^ round).collect();
            data = rc4(&key, &data);
        }
        data
    }
}

/// Algorithm 2: the file encryption key, for RC4 revisions 2..4.
fn compute_key(
    user_pw: &[u8],
    o: &[u8],
    p: i32,
    id0: &[u8],
    revision: u32,
    key_len: usize,
) -> Vec<u8> {
    let mut key = md5_of(&[&pad_password(user_pw), o, &p.to_le_bytes(), id0]).to_vec();
    if revision >= 3 {
        for _ in 0..50 {
            key = md5_of(&[&key[..key_len]]).to_vec();
        }
    }
    key.truncate(key_len);
    key
}

/// Algorithm 4 (revision 2) / Algorithm 5 (revisions 3, 4): the `/U` entry.
fn compute_u(key: &[u8], id0: &[u8], revision: u32) -> Vec<u8> {
    if revision == 2 {
        return rc4(key, &PAD);
    }
    let mut data = rc4(key, &md5_of(&[&PAD[..], id0]));
    for round in 1..=19u8 {
        let round_key: Vec<u8> = key.iter().map(|b| b ^ round).collect();
        data = rc4(&round_key, &data);
    }
    data.extend_from_slice(&[0u8; 16]);
    data
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

/// A binary string as a PDF literal string with octal escapes.
fn pdf_string(data: &[u8]) -> String {
    let mut s = String::from("(");
    for &b in data {
        match b {
            b'(' | b')' | b'\\' => {
                s.push('\\');
                s.push(b as char);
            }
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("\\{:03o}", b)),
        }
    }
    s.push(')');
    s
}

/// A document sealed with the standard security handler (empty user password).
/// [`Sealed::object`] produces the ciphertext of one object's strings/stream.
struct Sealed {
    o: Vec<u8>,
    u: Vec<u8>,
    key: Vec<u8>,
    id0: Vec<u8>,
}

impl Sealed {
    fn new(revision: u32, key_len: usize) -> Sealed {
        let id0 = b"0123456789abcdef".to_vec();
        let o = compute_o(b"", b"", revision, key_len);
        let key = compute_key(b"", &o, PERMS, &id0, revision, key_len);
        let u = compute_u(&key, &id0, revision);
        Sealed { o, u, key, id0 }
    }

    /// Algorithm 1: the per-object key, then RC4.
    fn object(&self, id: u32, gen: u16, data: &[u8]) -> Vec<u8> {
        let n = self.key.len();
        let mut seed = self.key.clone();
        seed.extend_from_slice(&id.to_le_bytes()[..3]);
        seed.extend_from_slice(&gen.to_le_bytes()[..2]);
        let key = md5_of(&[&seed]);
        rc4(&key[..(n + 5).min(16)], data)
    }
}

/// One-page document with an `/Encrypt` dictionary. `content` and `title` are
/// the bytes actually stored (the caller encrypts them or not, depending on
/// what the file claims); `enc_extra` is appended to the `/Encrypt` dictionary
/// (`/CF`, `/StmF`, `/StrF`), `stream_extra` to the content stream dictionary
/// (`/Filter [/Crypt]`, `/DecodeParms`).
fn crypt_pdf(
    sealed: &Sealed,
    v: i32,
    revision: u32,
    bits: u32,
    enc_extra: &str,
    stream_extra: &str,
    content: &[u8],
    title: &[u8],
) -> Vec<u8> {
    let contents = {
        let mut out =
            format!("<< /Length {} {} >>\nstream\n", content.len(), stream_extra).into_bytes();
        out.extend_from_slice(content);
        out.extend_from_slice(b"\nendstream");
        out
    };
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_vec(),
        contents,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        format!("<< /Title {} /Producer (pdf-rs spec cases) >>", pdf_string(title))
            .into_bytes(),
        format!(
            "<< /Filter /Standard /V {} /R {} /Length {} /O <{}> /U <{}> /P {}{} >>",
            v,
            revision,
            bits,
            hex(&sealed.o),
            hex(&sealed.u),
            PERMS,
            enc_extra
        )
        .into_bytes(),
    ];
    let id = hex(&sealed.id0);
    build_pdf_with_trailer(
        &objects,
        1,
        &format!(" /Info 6 0 R /ID [<{id}> <{id}>] /Encrypt 7 0 R"),
    )
}

const CONTENT: &[u8] = b"BT /F1 24 Tf 72 120 Td (Hello) Tj ET";

#[test]
fn fixtures_crypt_control_both_default() {
    // 7.6.5: /StmF and /StrF both point at the same RC4 filter. This file is
    // the control: mupdf and pdf-rs must both read text and title, which says
    // the generator (keys, /O, /U, object keys) is correct.
    let sealed = Sealed::new(4, 16);
    let content = sealed.object(4, 0, CONTENT);
    let title = sealed.object(6, 0, b"Crypt control");
    let bytes = crypt_pdf(
        &sealed,
        4,
        4,
        128,
        " /CF << /StdCF << /Type /CryptFilter /CFM /V2 /Length 16 >> >> \
          /StmF /StdCF /StrF /StdCF",
        "",
        &content,
        &title,
    );
    write_case("c20-crypt-control", &bytes);
    let r = pdf_rs_read(&bytes, "c20-crypt-control");
    assert_eq!(r.text, "Hello", "encrypted content, got {:?}", r);
    assert_eq!(r.title.as_deref(), Some("Crypt control"), "got {:?}", r);
}

#[test]
fn c21_strings_identity_streams_encrypted() {
    // 7.6.5 Table 20: /StrF is independent of /StmF. With /StrF /Identity the
    // strings are stored as plain text, so a reader that decrypts them anyway
    // returns garbage.
    let sealed = Sealed::new(4, 16);
    let content = sealed.object(4, 0, CONTENT);
    let bytes = crypt_pdf(
        &sealed,
        4,
        4,
        128,
        " /CF << /StdCF << /Type /CryptFilter /CFM /V2 /Length 16 >> >> \
          /StmF /StdCF /StrF /Identity",
        "",
        &content,
        b"ClearTitle",
    );
    write_case("c21-strf-identity", &bytes);
    let r = pdf_rs_read(&bytes, "c21-strf-identity");
    assert_eq!(r.text, "Hello", "got {:?}", r);
    assert_eq!(
        r.title.as_deref(),
        Some("ClearTitle"),
        "/StrF /Identity: the title is not encrypted and must not be decrypted, got {:?}",
        r
    );
}

#[test]
fn c22_streams_identity_strings_encrypted() {
    // 7.6.5 Table 20: /Identity is a legal value for /StmF, and then the
    // streams are plain while the strings use /StrF.
    let sealed = Sealed::new(4, 16);
    let title = sealed.object(6, 0, b"EncTitle");
    let bytes = crypt_pdf(
        &sealed,
        4,
        4,
        128,
        " /CF << /StdCF << /Type /CryptFilter /CFM /V2 /Length 16 >> >> \
          /StmF /Identity /StrF /StdCF",
        "",
        CONTENT,
        &title,
    );
    write_case("c22-stmf-identity", &bytes);
    let r = pdf_rs_read(&bytes, "c22-stmf-identity");
    assert_eq!(r.text, "Hello", "got {:?}", r);
    assert_eq!(r.title.as_deref(), Some("EncTitle"), "got {:?}", r);
}

#[test]
fn c23_stream_crypt_name_identity() {
    // 7.6.5: a `/Crypt` filter in the stream's /Filter array overrides the
    // default /StmF; `/Name /Identity` means this stream is not encrypted
    // while the rest of the file is.
    let sealed = Sealed::new(4, 16);
    let title = sealed.object(6, 0, b"EncTitle");
    let bytes = crypt_pdf(
        &sealed,
        4,
        4,
        128,
        " /CF << /StdCF << /Type /CryptFilter /CFM /V2 /Length 16 >> >> \
          /StmF /StdCF /StrF /StdCF",
        "/Filter [/Crypt] /DecodeParms [<< /Name /Identity >>]",
        CONTENT,
        &title,
    );
    write_case("c23-stream-name-identity", &bytes);
    let r = pdf_rs_read(&bytes, "c23-stream-name-identity");
    assert_eq!(
        r.text, "Hello",
        "/Name /Identity must leave this stream alone, got {:?}",
        r
    );
    assert_eq!(r.title.as_deref(), Some("EncTitle"), "got {:?}", r);
}

#[test]
fn c24_stream_crypt_name_from_cf() {
    // 7.6.5: `/Name` may select any filter from /CF, not just the default one.
    // /Plain has /CFM /None, i.e. it does nothing.
    let sealed = Sealed::new(4, 16);
    let title = sealed.object(6, 0, b"EncTitle");
    let bytes = crypt_pdf(
        &sealed,
        4,
        4,
        128,
        " /CF << /StdCF << /Type /CryptFilter /CFM /V2 /Length 16 >> \
          /Plain << /Type /CryptFilter /CFM /None >> >> \
          /StmF /StdCF /StrF /StdCF",
        "/Filter [/Crypt] /DecodeParms [<< /Name /Plain >>]",
        CONTENT,
        &title,
    );
    write_case("c24-stream-name-cf", &bytes);
    let r = pdf_rs_read(&bytes, "c24-stream-name-cf");
    assert_eq!(r.text, "Hello", "got {:?}", r);
    assert_eq!(r.title.as_deref(), Some("EncTitle"), "got {:?}", r);
}

#[test]
fn c25_strings_default_to_identity() {
    // 7.6.5 Table 20: /StrF is optional and defaults to /Identity. A file that
    // sets only /StmF has plain strings, so decrypting them breaks them.
    let sealed = Sealed::new(4, 16);
    let content = sealed.object(4, 0, CONTENT);
    let bytes = crypt_pdf(
        &sealed,
        4,
        4,
        128,
        " /CF << /StdCF << /Type /CryptFilter /CFM /V2 /Length 16 >> >> /StmF /StdCF",
        "",
        &content,
        b"ClearTitle",
    );
    write_case("c25-strf-absent", &bytes);
    let r = pdf_rs_read(&bytes, "c25-strf-absent");
    assert_eq!(r.text, "Hello", "got {:?}", r);
    assert_eq!(
        r.title.as_deref(),
        Some("ClearTitle"),
        "without /StrF the strings are /Identity, got {:?}",
        r
    );
}

// ---------------------------------------------------------------- wave 4: file structure (§7.5.8.4)

/// A hybrid-reference file (7.5.8.4): the update section is a classic table
/// whose trailer carries `/XRefStm`, and the object that only the
/// cross-reference stream knows about (8, a /PageLabels number tree) is marked
/// free with generation 65535 in the section that `/Prev` names.
fn hybrid_doc() -> Vec<u8> {
    let content = b"BT /F1 24 Tf 72 120 Td (Hello) Tj ET";
    let mut contents = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    contents.extend_from_slice(content);
    contents.extend_from_slice(b"\nendstream");

    let objects: Vec<(u32, Vec<u8>)> = vec![
        (
            1,
            b"<< /Type /Catalog /Pages 2 0 R /PageLabels 8 0 R >>".to_vec(),
        ),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] \
              /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
                .to_vec(),
        ),
        (4, contents),
        (
            5,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ),
        // the hidden object: reachable from the catalog, but it appears only in
        // the cross-reference stream
        (8, b"<< /Nums [0 << /S /D >>] >>".to_vec()),
    ];

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n");
    let mut pos = std::collections::HashMap::new();
    for (nr, body) in &objects {
        pos.insert(*nr, out.len() as u32);
        out.extend_from_slice(format!("{} 0 obj\n", nr).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }

    // main section: objects 6 and 7 do not exist yet, object 8 is free with
    // generation 65535 (that is how a hybrid file hides it)
    let main_xref = out.len() as u32;
    out.extend_from_slice(b"xref\n0 9\n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    for nr in 1..=5u32 {
        out.extend_from_slice(format!("{:010} 00000 n \n", pos[&nr]).as_bytes());
    }
    out.extend_from_slice(b"0000000000 00000 f \n0000000000 00000 f \n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    out.extend_from_slice(b"trailer\n<< /Size 9 /Root 1 0 R >>\n");
    out.extend_from_slice(format!("startxref\n{}\n%%EOF\n", main_xref).as_bytes());

    // cross-reference stream (object 6), /Index [8 1]: one in-use entry for 8
    let xrefstm_off = out.len() as u32;
    let mut data = vec![1u8];
    data.extend_from_slice(&pos[&8].to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes());
    let mut enc = Vec::new();
    for b in &data {
        enc.extend_from_slice(format!("{:02X}", b).as_bytes());
    }
    enc.push(b'>');
    out.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /XRef /Size 9 /Index [8 1] /W [1 4 2] \
             /Filter /ASCIIHexDecode /Length {} >>\nstream\n",
            enc.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&enc);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    // update section: classic table, /XRefStm names the stream above, /Prev
    // names the main table that marks object 8 free
    let update_xref = out.len() as u32;
    out.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \n6 1\n");
    out.extend_from_slice(format!("{:010} 00000 n \n", xrefstm_off).as_bytes());
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size 9 /Root 1 0 R /Prev {main_xref} /XRefStm {xrefstm_off} >>\n\
             startxref\n{update_xref}\n%%EOF\n"
        )
        .as_bytes(),
    );
    out
}

#[test]
fn c26_hybrid_xref_stream_entry_wins() {
    // 7.5.8.4: a hybrid-reference file hides an object by listing it as free
    // (generation 65535) in the previous section and as in use in the
    // cross-reference stream that /XRefStm names; the reader shall use the
    // stream entry and ignore the free one. The lookup order inside an update
    // section is: the section itself, then its /XRefStm, then /Prev.
    use pdf::object::{PlainRef, Resolve};

    let bytes = hybrid_doc();
    write_case("c26-hybrid-xrefstm", &bytes);

    let r = pdf_rs_read(&bytes, "c26-hybrid-xrefstm");
    assert_eq!(r.text, "Hello", "hybrid file must still render, got {:?}", r);

    let file = pdf::file::FileOptions::uncached()
        .load(&bytes[..])
        .expect("c26: hybrid file must open");
    let resolver = file.resolver();
    let hidden = resolver
        .resolve(PlainRef { id: 8, gen: 0 })
        .expect("the entry from /XRefStm must not be replaced by the older free entry");
    let dict = hidden.into_dictionary().expect("object 8 is a dictionary");
    assert!(
        dict.contains_key("Nums"),
        "object 8 must be the number tree, got {:?}",
        dict
    );
}

// ---------------------------------------------------------------- wave 6: fonts (§9.6.2.1)

/// One page with a single font: object 5 is the font dictionary, object 6 its
/// `/FontDescriptor`, both given as raw PDF syntax.
fn font_doc(font: &str, descriptor: &str, content: &[u8]) -> Vec<u8> {
    let contents = {
        let mut v = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        v.extend_from_slice(content);
        v.extend_from_slice(b"\nendstream");
        v
    };
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_vec(),
        contents,
        font.as_bytes().to_vec(),
        descriptor.as_bytes().to_vec(),
    ];
    build_pdf(&objects, 1)
}

/// Widths pdf-rs reports for `codes` in the single font `/F1` of page 1.
fn font_widths(bytes: &[u8], name: &str, codes: &[usize]) -> Vec<f32> {
    let file = pdf::file::FileOptions::uncached()
        .load(bytes)
        .unwrap_or_else(|e| panic!("{}: pdf-rs cannot open the file: {}", name, e));
    let resolver = file.resolver();
    let page = file
        .get_page(0)
        .unwrap_or_else(|e| panic!("{}: page 0: {}", name, e));
    let res = page
        .resources()
        .unwrap_or_else(|e| panic!("{}: resources: {}", name, e));
    let lazy = res
        .data()
        .fonts
        .get(&Name::from("F1"))
        .unwrap_or_else(|| panic!("{}: /F1 missing from /Resources /Font", name));
    let font = lazy
        .load(&resolver)
        .unwrap_or_else(|e| panic!("{}: font /F1: {}", name, e));
    let widths = font
        .data()
        .widths(&resolver)
        .unwrap_or_else(|e| panic!("{}: widths: {}", name, e));
    let widths = widths.unwrap_or_else(|| panic!("{}: no widths at all", name));
    codes.iter().map(|&c| widths.get(c)).collect()
}

const HELVETICA_DESCRIPTOR: &str = "<< /Type /FontDescriptor /FontName /Helvetica /Flags 32 \
     /FontBBox [ -166 -225 1000 931 ] /ItalicAngle 0 /Ascent 718 /Descent -207 \
     /CapHeight 718 /StemV 88 /MissingWidth 250 >>";

#[test]
fn c27_missing_width_outside_widths_range() {
    // 9.6.2.1 Table 111 (/Widths): "For character codes outside the range
    // FirstChar to LastChar, the value of MissingWidth from the FontDescriptor
    // entry for this font shall be used."
    let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
                /FirstChar 65 /LastChar 65 /Widths [ 500 ] /FontDescriptor 6 0 R >>";
    let bytes = font_doc(font, HELVETICA_DESCRIPTOR, b"BT /F1 12 Tf 72 120 Td (Helo) Tj ET");
    write_case("c27-missing-width", &bytes);

    let r = pdf_rs_read(&bytes, "c27-missing-width");
    assert_eq!(r.text, "Helo", "the text must still be read, got {:?}", r);

    // 'A' is inside FirstChar..LastChar, 'H' and ' ' (after the string) are not
    let got = font_widths(&bytes, "c27-missing-width", &[65, 72, 32]);
    assert_eq!(
        got,
        vec![500.0, 250.0, 250.0],
        "codes outside /FirstChar../LastChar take /MissingWidth"
    );
}

#[test]
fn c27b_missing_width_without_widths_array() {
    // same rule when /Widths is absent entirely: every code is "not specified
    // in a font dictionary's Widths array" (Table 122 /MissingWidth)
    let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
                /FirstChar 65 /LastChar 65 /FontDescriptor 6 0 R >>";
    let bytes = font_doc(font, HELVETICA_DESCRIPTOR, b"BT /F1 12 Tf 72 120 Td (Helo) Tj ET");
    write_case("c27b-missing-width-no-widths", &bytes);

    let r = pdf_rs_read(&bytes, "c27b-missing-width-no-widths");
    assert_eq!(r.text, "Helo", "the text must still be read, got {:?}", r);

    let got = font_widths(&bytes, "c27b-missing-width-no-widths", &[65, 32]);
    assert_eq!(
        got,
        vec![250.0, 250.0],
        "without /Widths every code takes /MissingWidth"
    );
}

#[test]
fn c28_type0_without_descendant_font() {
    // 9.7.4: /DescendantFonts "shall be an array of a single ... entry"; Table
    // 110 makes it required for Type 0 fonts. pdf-rs indexed `[0]` into it and
    // panicked on an empty or absent array - malformed input must come back as
    // an error, not as an index-out-of-bounds panic.
    for (name, font) in [
        (
            "c28-type0-empty-descendants",
            "<< /Type /Font /Subtype /Type0 /BaseFont /Foo /Encoding /Identity-H \
               /DescendantFonts [ ] >>",
        ),
        (
            "c28b-type0-no-descendants",
            "<< /Type /Font /Subtype /Type0 /BaseFont /Foo /Encoding /Identity-H >>",
        ),
    ] {
        let bytes = font_doc(font, HELVETICA_DESCRIPTOR, b"BT /F1 12 Tf 72 120 Td (A) Tj ET");
        write_case(name, &bytes);

        let file = pdf::file::FileOptions::uncached()
            .load(&bytes[..])
            .unwrap_or_else(|e| panic!("{}: pdf-rs cannot open the file: {}", name, e));
        let resolver = file.resolver();
        let page = file
            .get_page(0)
            .unwrap_or_else(|e| panic!("{}: page 0: {}", name, e));
        let res = page
            .resources()
            .unwrap_or_else(|e| panic!("{}: resources: {}", name, e));
        let lazy = res
            .data()
            .fonts
            .get(&Name::from("F1"))
            .unwrap_or_else(|| panic!("{}: /F1 missing from /Resources /Font", name));
        let font = match lazy.load(&resolver) {
            Ok(f) => f,
            // rejected while parsing the font dictionary: an error already,
            // and not a panic - that is fine
            Err(_) => continue,
        };

        let data = font.data();
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| data.widths(&resolver)));
        match outcome {
            Err(_) => panic!(
                "{}: pdf-rs panicked on a Type 0 font without /DescendantFonts",
                name
            ),
            Ok(Err(_)) => {}
            Ok(Ok(w)) => panic!("{}: expected an error, got widths {:?}", name, w),
        }
    }
}

#[test]
fn c29_widths_do_not_extend_past_last_char() {
    // 9.6.2.1 (Table 111, /Widths): the array holds exactly
    // LastChar - FirstChar + 1 entries and only covers FirstChar..LastChar;
    // "for character codes outside the range FirstChar to LastChar, the value
    // of MissingWidth ... shall be used". pdf-rs bounded the range by
    // FirstChar + /Widths length instead of /LastChar, so a surplus entry was
    // used for a code above /LastChar.
    let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
                /FirstChar 65 /LastChar 66 /Widths [ 500 600 700 ] /FontDescriptor 6 0 R >>";
    let bytes = font_doc(font, HELVETICA_DESCRIPTOR, b"BT /F1 12 Tf 72 120 Td (AH) Tj ET");
    write_case("c29-widths-beyond-last-char", &bytes);

    let r = pdf_rs_read(&bytes, "c29-widths-beyond-last-char");
    assert_eq!(r.text, "AH", "the text must still be read, got {:?}", r);

    // 65 'A', 66 'B' are in range; 67 is inside the array but past
    // /LastChar, 32 is below /FirstChar.
    let got = font_widths(&bytes, "c29-widths-beyond-last-char", &[65, 66, 67, 32]);
    assert_eq!(
        got,
        vec![500.0, 600.0, 250.0, 250.0],
        "only /FirstChar../LastChar indexes /Widths; the third entry is surplus"
    );
}

#[test]
fn c29b_short_widths_array_falls_back_to_missing_width() {
    // A /Widths array shorter than LastChar - FirstChar + 1 is malformed, but
    // the code it does not cover still has "its width not specified in a font
    // dictionary's Widths array", so Table 122 /MissingWidth applies. This
    // passes both before and after the /LastChar fix: it guards against the
    // fix turning the short-array case into an error or a zero.
    let font = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
                /FirstChar 65 /LastChar 67 /Widths [ 500 600 ] /FontDescriptor 6 0 R >>";
    let bytes = font_doc(font, HELVETICA_DESCRIPTOR, b"BT /F1 12 Tf 72 120 Td (AH) Tj ET");
    write_case("c29b-short-widths-array", &bytes);

    let got = font_widths(&bytes, "c29b-short-widths-array", &[65, 66, 67]);
    assert_eq!(
        got,
        vec![500.0, 600.0, 250.0],
        "a code the array does not cover takes /MissingWidth"
    );
}

// ------------------------------------------------- wave 7: file structure (§7.5.1, 7.5.5)

/// One page with one line of text, as a complete file.
fn small_doc() -> Vec<u8> {
    text_doc(b"BT /F1 12 Tf 72 120 Td (Hi) Tj ET", "F1")
}

#[test]
fn c30_header_version_is_validated() {
    // 7.5.1: "The PDF file begins with a header containing a version number
    // of the form %PDF-n.m". pdf-rs took whatever followed `%PDF-` as the
    // version string, so a file whose version digits were damaged was read
    // without complaint.
    let good = small_doc();
    let file = pdf::file::FileOptions::uncached()
        .load(&good[..])
        .expect("c30: the untouched document must open");
    assert_eq!(
        file.version().expect("c30: version of a conforming header"),
        "PDF-1.7",
        "the version string is the header without the leading %"
    );

    let mut bad = good.clone();
    bad[..8].copy_from_slice(b"%PDF-#!?");
    write_case("c30-header-version", &bad);

    // strict parsing: the damage is reported, and as an error from `load`
    match pdf::file::FileOptions::uncached().load(&bad[..]) {
        Ok(_) => panic!("c30: a header that is not %PDF-n.m must be rejected"),
        Err(e) => assert!(
            e.to_string().contains("%PDF-n.m"),
            "c30: the error should name the header, got {}",
            e
        ),
    }

    // tolerant parsing: recovery, Annex C — the page is still readable
    let file = pdf::file::FileOptions::uncached()
        .parse_options(ParseOptions::tolerant())
        .load(&bad[..])
        .expect("c30: a tolerant reader must still open a damaged header");
    assert_eq!(file.num_pages(), 1);
}

#[test]
fn c31_eof_marker_is_required() {
    // 7.5.5: "The last line of the file shall contain only the end-of-file
    // marker, %%EOF." Nothing checked it: a truncated file (no marker at all)
    // and a file with junk after the marker both loaded as if complete.
    let good = small_doc();
    assert!(
        good.ends_with(b"%%EOF\n"),
        "c31: the fixture builder must end the file with %%EOF"
    );
    let mut truncated = good.clone();
    truncated.truncate(good.len() - b"%%EOF\n".len());
    let mut padded = good.clone();
    padded.extend_from_slice(b"this is not part of the document\n");

    for (name, bytes, why) in [
        ("c31-eof-missing", truncated, "the file ends after startxref"),
        ("c31b-eof-trailing-data", padded, "data follows the marker"),
    ] {
        write_case(name, &bytes);

        match pdf::file::FileOptions::uncached().load(&bytes[..]) {
            Ok(_) => panic!("{}: strict parsing must reject a file where {}", name, why),
            Err(e) => assert!(
                e.to_string().contains("%%EOF"),
                "{}: the error should name the marker, got {}",
                name,
                e
            ),
        }

        // recovery: a tolerant reader still gets the page back
        let r = {
            let file = pdf::file::FileOptions::uncached()
                .parse_options(ParseOptions::tolerant())
                .load(&bytes[..])
                .unwrap_or_else(|e| panic!("{}: tolerant open failed: {}", name, e));
            let resolver = file.resolver();
            let page = file.get_page(0).expect("page 0");
            let res = page.resources().expect("resources");
            let content = page.contents.as_ref().expect("contents");
            let mut text = String::new();
            for op in content.operations(&resolver).expect("operations") {
                if let pdf::content::Op::TextDraw { text: t } = op {
                    text.push_str(&String::from_utf8_lossy(&t.data));
                }
            }
            let _ = res;
            text
        };
        assert_eq!(r, "Hi", "{}: the page must survive recovery", name);
    }
}

#[test]
fn c32_open_refuses_oversized_files() {
    // `File::open` reads the whole file into memory with no ceiling. It now
    // has one, and a caller can set it.
    let bytes = small_doc();
    let path = write_case("c32-open-size-limit", &bytes);

    let err = match pdf::file::FileOptions::uncached()
        .max_file_size(Some(64))
        .open(&path)
    {
        Ok(_) => panic!("c32: a 64 byte limit must refuse a larger file"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("limit"),
        "c32: the error should explain the limit, got {}",
        err
    );

    // the limit is optional, and a big enough one opens the same file
    let file = pdf::file::FileOptions::uncached()
        .max_file_size(None)
        .open(&path)
        .expect("c32: without a limit the file opens");
    assert_eq!(file.num_pages(), 1);
}

/// Text drawn by `Tj` on page 0, parsed with the given options.
fn page_text_with(bytes: &[u8], opts: ParseOptions) -> pdf::error::Result<String> {
    use pdf::content::Op;

    let file = pdf::file::FileOptions::uncached()
        .parse_options(opts)
        .load(bytes)?;
    let resolver = file.resolver();
    let page = file.get_page(0)?;
    let mut text = String::new();
    if let Some(content) = &page.contents {
        for op in content.operations(&resolver)? {
            if let Op::TextDraw { text: t } = op {
                text.push_str(&String::from_utf8_lossy(&t.data));
            }
        }
    }
    Ok(text)
}

#[test]
fn c33_unknown_operator_fails_in_strict_and_is_skipped_in_tolerant() {
    // 7.8.2: an operator the consumer does not know (an extension, a vendor
    // prefix) may be skipped by a consumer that recovers, but it is not
    // conforming; `ParseOptions::strict()` used to tolerate it just like
    // `tolerant()` did, so no caller could ask for the strict behaviour.
    let content = b"BT /F1 12 Tf 72 120 Td (Hi) Tj ET\n0 vendorop\n";
    let bytes = text_doc(content, "F1");
    let path = write_case("c33-unknown-operator", &bytes);
    assert!(path.exists());

    match page_text_with(&bytes, ParseOptions::strict()) {
        Ok(text) => panic!(
            "c33: strict parsing must report `vendorop`, not skip it (got {:?})",
            text
        ),
        Err(e) => println!("c33: strict error as expected: {}", e),
    }

    let tolerant = page_text_with(&bytes, ParseOptions::tolerant())
        .unwrap_or_else(|e| panic!("c33: tolerant parsing must skip `vendorop`: {}", e));
    assert_eq!(tolerant, "Hi", "c33: the text before the operator is kept");

    // 7.8.1: inside BX ... EX (a compatibility section) unknown operators are
    // ignored, so even strict parsing accepts the same stream.
    let content = b"BT /F1 12 Tf 72 120 Td (Hi) Tj ET\nBX 0 vendorop EX\n";
    let bytes = text_doc(content, "F1");
    write_case("c33b-unknown-operator-in-bx", &bytes);
    let text = page_text_with(&bytes, ParseOptions::strict())
        .unwrap_or_else(|e| panic!("c33b: BX ... EX must guard unknown operators: {}", e));
    assert_eq!(text, "Hi", "c33b: text is still drawn in the section");

    // the fixture is a readable one-page document for an outside viewer
    assert_eq!(
        pdf_rs_read(&bytes, "c33b-unknown-operator-in-bx").text,
        "Hi"
    );
}

/// One-page document whose text uses a Type 3 font with the given glyph
/// procedures (`/A` for code 65, `/B` for code 66).
fn type3_doc(glyph_proc: &[u8]) -> Vec<u8> {
    let stream = |data: &[u8]| {
        let mut v = format!("<< /Length {} >>\nstream\n", data.len()).into_bytes();
        v.extend_from_slice(data);
        v.extend_from_slice(b"\nendstream");
        v
    };
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_vec(),
        stream(b"BT /F1 60 Tf 72 100 Td (AB) Tj ET"),
        b"<< /Type /Font /Subtype /Type3 /Name /F1 /FontBBox [0 0 700 700] \
          /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs 6 0 R \
          /Encoding << /Type /Encoding /Differences [65 /A /B] >> \
          /FirstChar 65 /LastChar 66 /Widths [700 700] /Resources << >> >>"
            .to_vec(),
        b"<< /A 7 0 R /B 8 0 R >>".to_vec(),
        stream(glyph_proc),
        stream(glyph_proc),
    ];
    build_pdf(&objects, 1)
}

#[test]
fn c34_type3_glyph_width_operator_d0() {
    // 9.6.5 / Table 113: `d0` sets the glyph width (operands wx wy, and wy
    // shall be 0) and paints with the text state colour. It occurs only in a
    // Type 3 font's charproc, so dropping it loses nothing on an ordinary
    // page — but an op stream that is read and written back lost it silently.
    use pdf::content::{parse_ops, serialize_ops, Op};

    let ops = parse_ops(b"700 0 d0", &NoResolve).expect("c34: d0 must parse");
    assert!(
        matches!(ops.as_slice(), [Op::GlyphWidth { wx, wy }] if *wx == 700.0 && *wy == 0.0),
        "c34: d0 must survive parsing, got {:?}",
        ops
    );
    assert_eq!(
        serialize_ops(&ops).expect("c34: d0 must serialize"),
        b"700 0 d0\n".to_vec(),
        "c34: `d0` must be written back"
    );

    // a whole document that really uses it: the glyph procedures are filled
    // boxes, instrumentation from pdf-rs only reads the page, so the fixture
    // is for the outside viewer.
    let bytes = type3_doc(b"700 0 d0\n0 0 700 700 re f");
    let path = write_case("c34-type3-d0", &bytes);
    assert!(path.exists());
    assert_eq!(
        pdf_rs_read(&bytes, "c34-type3-d0").text,
        "AB",
        "c34: the text still comes from the page content stream"
    );
}

#[test]
fn c35_type3_glyph_width_and_bbox_operator_d1() {
    // 9.6.5 / Table 113: `d1` sets the glyph width (wx wy) and the glyph
    // bounding box (llx lly urx ury); the charproc paints its own colour.
    // Like `d0` it was parsed and dropped, so a read/write round trip of a
    // charproc lost the operator and with it the declared metrics.
    use pdf::content::{parse_ops, serialize_ops, Op};

    let ops = parse_ops(b"700 0 0 0 700 700 d1", &NoResolve).expect("c35: d1 must parse");
    assert!(
        matches!(
            ops.as_slice(),
            [Op::GlyphWidthAndBBox { wx, wy, bbox }]
                if *wx == 700.0 && *wy == 0.0
                    && bbox.x == 0.0 && bbox.y == 0.0
                    && bbox.width == 700.0 && bbox.height == 700.0
        ),
        "c35: d1 must survive parsing, got {:?}",
        ops
    );
    assert_eq!(
        serialize_ops(&ops).expect("c35: d1 must serialize"),
        b"700 0 0 0 700 700 d1\n".to_vec(),
        "c35: `d1` must be written back"
    );

    // a document whose glyph procedures use d1 and paint blue themselves
    let bytes = type3_doc(b"0 0 1 rg 700 0 0 0 700 700 d1\n0 0 700 700 re f");
    let path = write_case("c35-type3-d1", &bytes);
    assert!(path.exists());
    assert_eq!(
        pdf_rs_read(&bytes, "c35-type3-d1").text,
        "AB",
        "c35: the text still comes from the page content stream"
    );
}

