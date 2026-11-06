//! Regression tests for the hostile-input and robustness fixes in this branch.
//!
//! Every test here fails (or, for the allocation tests, dies) on unpatched
//! master and passes after the fix it is named for. The PDFs are built by hand
//! so the malformed structure is exactly what the test is about.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use pdf::file::{CachedFile, FileOptions, UpdateOptions};
use pdf::object::{NameTree, Object, PlainRef, Updater};
use pdf::primitive::Primitive;

// ------------------------------------------------------------------ helper

/// A minimal PDF writer: objects are given as their body text, in object
/// number order starting at 1, `None` for a free entry. The cross-reference
/// table is written with real offsets, so the file parses with the strict
/// parser.
fn build_pdf(objects: &[Option<String>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n");
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize); // object 0 is always free
    for (i, body) in objects.iter().enumerate() {
        match body {
            Some(body) => {
                offsets.push(out.len());
                out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
            }
            None => offsets.push(0),
        }
    }
    let xref_at = out.len();
    let size = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets[1..] {
        if *offset == 0 {
            out.extend_from_slice(b"0000000000 00000 f \n");
        } else {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n").as_bytes(),
    );
    out
}

fn tmp_pdf(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join("pdf-rs-regressions");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn load(objects: &[Option<String>]) -> CachedFile<Vec<u8>> {
    // A unique path per call: the test binary runs these in parallel and a
    // shared file name would have them overwrite each other.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let name = format!("regress-{}.pdf", NEXT.fetch_add(1, Ordering::SeqCst));
    let path = tmp_pdf(&name, &build_pdf(objects));
    FileOptions::cached().open(path).expect("load")
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

/// Catalog + Pages + one page, object 4 free.
fn one_page_with_free_object_4(rotate: &str) -> Vec<Option<String>> {
    vec![
        s("<< /Type /Catalog /Pages 2 0 R >>"),
        s("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        s(&format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Rotate {rotate} >>"
        )),
        None,
    ]
}

// --------------------------------------------- 7.3.3 /Rotate normalisation

#[test]
fn page_rotate_is_normalised_modulo_360() {
    // 7.3.3: /Rotate is the number of degrees by which the page shall be
    // rotated clockwise when displayed. It is a multiple of 90 (7.7.3.3), and
    // a writer that emitted 450 means 90. Anything else is ignored.
    for (input, expected) in [("450", 90), ("-90", 270), ("360", 0), ("45", 0), ("-45", 0)] {
        let file = load(&one_page_with_free_object_4(input));
        let page = file.get_page(0).expect("page");
        assert_eq!(
            page.rotate(),
            expected,
            "/Rotate {input} must read as {expected}"
        );
    }
}

// ------------------------------------------------------ 7.5.4 free objects

#[test]
fn updating_a_free_object_is_an_error_not_a_panic() {
    // 7.5.4: a free entry has no object. Writing into it used to panic!().
    let mut file = load(&one_page_with_free_object_4("0"));
    let free = PlainRef { id: 4, gen: 0 };
    let result = file.update_with(free, Primitive::Null, UpdateOptions::default());
    assert!(result.is_err(), "updating a free object must fail, got {result:?}");
    assert!(
        result.unwrap_err().to_string().contains("free"),
        "the error should say the object is free"
    );
}

#[test]
fn updating_an_object_beyond_size_is_an_error() {
    // An object beyond /Size has no entry at all; that is an error, not a panic.
    let mut file = load(&one_page_with_free_object_4("0"));
    let beyond = PlainRef { id: 99, gen: 0 };
    let result = file.update_with(beyond, Primitive::Null, UpdateOptions::default());
    match result {
        Err(pdf::PdfError::NullRef { obj_nr }) => assert_eq!(obj_nr, 99),
        Err(pdf::PdfError::UnspecifiedXRefEntry { id }) => assert_eq!(id, 99),
        other => panic!("expected a bad-xref error, got {other:?}"),
    }
}

// ------------------------------------------------------- 7.9.6 name trees

/// Catalog, page tree, page, then a name tree rooted at object 4 whose `/Kids`
/// is `kids`.
fn name_tree_pdf(kids: &str) -> Vec<Option<String>> {
    vec![
        s("<< /Type /Catalog /Pages 2 0 R >>"),
        s("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        s("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>"),
        s(&format!("<< /Kids [{kids}] /Limits [(a) (z)] >>")),
    ]
}

fn walk_names(objects: &[Option<String>]) -> pdf::error::Result<usize> {
    let file = load(objects);
    let tree = NameTree::<Primitive>::from_primitive(
        Primitive::Reference(PlainRef { id: 4, gen: 0 }),
        &file,
    )?;
    let mut count = 0;
    tree.walk(&file, &mut |_name, _value| count += 1)?;
    Ok(count)
}

#[test]
fn name_tree_that_points_back_at_itself_is_rejected() {
    // 7.9.6: a name tree is a tree; a /Kids entry that leads back to a node
    // already entered is a false document. It used to recurse until the stack
    // (or the allocator) gave out.
    let err = walk_names(&name_tree_pdf("4 0 R")).expect_err("self-cycle must be rejected");
    assert!(
        err.to_string().contains("more than once"),
        "unexpected error: {err}"
    );
}

#[test]
fn name_tree_cycle_between_two_nodes_is_rejected() {
    let mut objects = name_tree_pdf("5 0 R");
    objects.push(s("<< /Kids [4 0 R] >>"));
    let err = walk_names(&objects).expect_err("cycle must be rejected");
    assert!(
        err.to_string().contains("more than once"),
        "unexpected error: {err}"
    );
}

#[test]
fn name_tree_depth_is_bounded() {
    // A straight 200-deep chain is not a cycle, but it is still deeper than
    // any real file and must not exhaust the stack.
    let depth = 200;
    let mut objects = name_tree_pdf("5 0 R");
    for i in 5..=(4 + depth) {
        objects.push(s(&format!("<< /Kids [{} 0 R] >>", i + 1)));
    }
    objects.push(s("<< /Names [(a) 1] >>"));
    let err = walk_names(&objects).expect_err("a 200-deep chain must be rejected");
    assert!(
        err.to_string().contains("deeper than"),
        "unexpected error: {err}"
    );
}

#[test]
fn well_formed_name_tree_walks() {
    // The guard must not reject a normal tree: root -> two leaves.
    let mut objects = name_tree_pdf("5 0 R 6 0 R");
    objects.push(s("<< /Names [(a) 1 (b) 2] >>"));
    objects.push(s("<< /Names [(c) 3] >>"));
    assert_eq!(walk_names(&objects).unwrap(), 3);
}
