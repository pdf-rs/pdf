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

// `/Rotate` is an inheritable page attribute: a Page with no `/Rotate` of its
// own takes the value from the nearest ancestor `Pages` node. This builds a
// minimal one-page document whose rotation lives only on the page tree and
// asserts the page reports it.
#[cfg(feature = "cache")]
#[test]
fn rotate_is_inherited() {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] /Rotate 90 >>",
        "<< /Type /Page /Parent 2 0 R >>",
    ];
    let mut pdf = String::from("%PDF-1.5\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj {} endobj\n", i + 1, body));
    }
    let startxref = pdf.len();
    // Classic cross-reference table; each entry line is exactly 20 bytes.
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
    let page = run!(file.get_page(0));
    assert!(page.rotate.is_none(), "the page itself must not carry /Rotate");
    assert_eq!(page.rotate(), 90, "page should inherit /Rotate from its parent Pages node");
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

// `/P` written as the unsigned 32-bit value (4294967292 instead of -4), see #293.
#[test]
fn unsigned_permissions() {
    let file = run!(FileOptions::uncached()
        .password(b"foo")
        .open(file_path("unsigned_p/bad-encrypted.pdf")));
    for i in 0..file.num_pages() {
        let _ = run!(file.get_page(i));
    }
}

// /Contents as an indirect reference to an array of content streams, see #297.
fn contents_reference_to_array_pdf() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>",
        "[5 0 R 6 0 R]",
        "<< /Length 4 >>\nstream\n0 g \nendstream",
        "<< /Length 4 >>\nstream\n1 g \nendstream",
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objs.iter().enumerate() {
        offsets.push(data.len());
        data.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).bytes());
    }
    let xref = data.len();
    data.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
    for o in offsets {
        data.extend(format!("{:010} 00000 n \n", o).bytes());
    }
    data.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", objs.len() + 1, xref).bytes());
    data
}

#[test]
fn contents_reference_to_array() {
    for options in [ParseOptions::strict(), ParseOptions::tolerant()] {
        let file = run!(FileOptions::uncached()
            .parse_options(options)
            .load(contents_reference_to_array_pdf()));
        let page = run!(file.get_page(0));
        // in tolerant mode a parse error would show up here as `None`
        let content = page.contents.as_ref().expect("contents");
        assert_eq!(content.parts.len(), 2);
    }
}

// #289: a Type 0 function whose /Encode overflows f32 to infinity used to panic in `apply`.
#[test]
fn sampled_function_infinite_encode() {
    use pdf::object::ColorSpace;

    let file = run!(FileOptions::uncached().open(file_path("invalid/289-crash.pdf")));
    let page = run!(file.get_page(0));
    let resources = run!(page.resources());
    let cs = resources.color_spaces.get("CS1").expect("CS1");
    let ColorSpace::Separation(_, _, tint) = cs else {
        panic!("expected Separation, got {:?}", cs);
    };
    let mut out = [0.0; 2];
    run!(tint.apply(&[0.5], &mut out));
    assert!(out.iter().all(|y| (0.0..=1.0).contains(y)), "{:?}", out);
}
