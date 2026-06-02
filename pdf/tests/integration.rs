use pdf::file::FileOptions;
use pdf::object::*;
use pdf::parser::{parse, ParseFlags};
use std::path::{Path, PathBuf};
use std::str;

macro_rules! run {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => {
                panic!("{}", e);
            }
        }
    };
}

fn files() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("files")
}
fn file_path(s: &str) -> PathBuf {
    files().join(s)
}
fn dir_pdfs(path: PathBuf) -> impl Iterator<Item = PathBuf> {
    path.read_dir()
        .unwrap()
        .filter_map(|r| r.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "pdf").unwrap_or(false))
}

#[test]
fn open_file() {
    let _ = run!(FileOptions::uncached().open(file_path("example.pdf")));
    #[cfg(all(feature = "mmap", feature = "cache"))]
    let _ = run!({
        use memmap2::Mmap;
        let file = std::fs::File::open(file_path!("example.pdf")).expect("can't open file");
        let mmap = unsafe { Mmap::map(&file).expect("can't mmap file") };
        FileOptions::cached().load(mmap)
    });
}

#[cfg(feature = "cache")]
#[test]
fn read_pages() {
    for path in dir_pdfs(files()) {
        println!("\n == Now testing `{}` ==", path.to_str().unwrap());

        let path = path.to_str().unwrap();
        let file = run!(FileOptions::cached().open(path));
        for i in 0..file.num_pages() {
            println!("Read page {}", i);
            let _ = file.get_page(i);
        }
    }
}

#[test]
fn user_password() {
    for path in dir_pdfs(file_path("password_protected")) {
        println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

        let path = path.to_str().unwrap();
        let file = run!(FileOptions::uncached().password(b"userpassword").open(path));
        for i in 0..file.num_pages() {
            println!("\nRead page {}", i);
            let _ = file.get_page(i);
        }
    }
}

#[test]
fn owner_password() {
    for path in dir_pdfs(file_path("password_protected")) {
        println!("\n\n == Now testing `{}` ==\n", path.to_str().unwrap());

        let path = path.to_str().unwrap();
        let file = run!(FileOptions::uncached()
            .password(b"ownerpassword")
            .open(path));
        for i in 0..file.num_pages() {
            println!("\nRead page {}", i);
            let _ = file.get_page(i);
        }
    }
}

// Decrypting a *stream* exercises the file key end-to-end. Opening and walking
// page dictionaries isn't enough — PDF encrypts only strings and streams, not
// dictionary structure, so a wrong/truncated key still parses dicts fine. This
// regresses the AES-256 (AESV3) key-truncation bug, which only showed when a
// stream was actually decrypted (object stream or content stream).
#[cfg(feature = "cache")]
#[test]
fn decrypt_streams() {
    let mut decoded_any = false;
    for path in dir_pdfs(file_path("password_protected")) {
        let path = path.to_str().unwrap();
        println!("\n == decrypting streams in `{}` ==", path);
        let file = run!(FileOptions::cached().password(b"userpassword").open(path));
        let resolver = file.resolver();
        for i in 0..file.num_pages() {
            let page = run!(file.get_page(i));
            if let Some(content) = &page.contents {
                // Forces decrypt + filter-decode of the content stream; a bad
                // key yields garbage that fails to inflate/parse here.
                let ops = run!(content.operations(&resolver));
                decoded_any |= !ops.is_empty();
            }
        }
    }
    assert!(decoded_any, "no content stream was decoded — test exercised nothing");
}

// A Type 3 font should parse into the typed `Type3Font` (font matrix, glyph
// procedures, widths) rather than being dumped into `FontData::Other`.
#[cfg(feature = "cache")]
#[test]
fn type3_font_parses() {
    let glyph = "1000 0 0 0 750 750 d1"; // a Type 3 glyph procedure
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type3 /FontMatrix [0.001 0 0 0.001 0 0] \
         /FontBBox [0 0 750 750] /CharProcs 5 0 R \
         /Encoding << /Type /Encoding /Differences [65 /a] >> \
         /FirstChar 65 /LastChar 65 /Widths [600] >>"
            .to_string(),
        "<< /a 6 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", glyph.len(), glyph),
    ];
    let mut pdf = String::from("%PDF-1.5\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj {} endobj\n", i + 1, body));
    }
    let startxref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    pdf.push_str("0000000000 65535 f \n");
    for off in &offsets {
        pdf.push_str(&format!("{off:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer << /Root 1 0 R /Size {} >>\nstartxref\n{}\n%%EOF",
        objects.len() + 1,
        startxref
    ));

    let file = run!(FileOptions::cached().load(pdf.into_bytes()));
    let resolver = file.resolver();
    let font: RcRef<pdf::font::Font> = run!(resolver.get(Ref::new(PlainRef { id: 4, gen: 0 })));
    let t3 = font.type3().expect("the font should parse as a typed Type 3 font");
    // Parsing CharProcs as RcRef<Stream<()>> proves the glyph stream resolved.
    assert_eq!(t3.char_procs.len(), 1, "one glyph procedure");
    assert_eq!(t3.first_char, Some(65));
    assert_eq!(t3.widths.as_deref(), Some([600.0].as_slice()));
    assert!((t3.font_matrix.a - 0.001).abs() < 1e-6, "font matrix preserved");
}

// Test for invalid PDFs found by fuzzing.
// We don't care if they give an Err or Ok, as long as they don't panic.
#[cfg(feature = "cache")]
#[test]
fn invalid_pdfs() {
    for path in dir_pdfs(file_path("invalid")) {
        let path = path.to_str().unwrap();
        println!("\n\n == Now testing `{}` ==\n", path);

        match FileOptions::cached().open(path) {
            Ok(file) => {
                for i in 0..file.num_pages() {
                    let _ = file.get_page(i);
                }
            }
            Err(_) => {
                continue;
            }
        }
    }
}

#[cfg(feature = "cache")]
#[test]
fn parse_objects_from_stream() {
    use pdf::object::NoResolve;
    let file = run!(FileOptions::cached().open(file_path("xelatex.pdf")));
    let resolver = file.resolver();

    // .. we know that object 13 of that file is an ObjectStream
    let obj_stream: RcRef<ObjectStream> = run!(resolver.get(Ref::new(PlainRef { id: 13, gen: 0 })));
    for i in 0..obj_stream.n_objects() {
        let (data, range) = run!(obj_stream.get_object_slice(i, &resolver));
        let slice = &data[range];
        println!("Object slice #{}: {}\n", i, str::from_utf8(slice).unwrap());
        run!(parse(slice, &NoResolve, ParseFlags::ANY));
    }
}

// TODO test decoding
