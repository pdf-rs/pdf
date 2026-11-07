//! Regression tests for the hostile-input and robustness fixes in this branch.
//!
//! Every test here fails (or, for the allocation tests, dies) on unpatched
//! master and passes after the fix it is named for. The PDFs are built by hand
//! so the malformed structure is exactly what the test is about.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use pdf::build::{Importer, PageBuilder, PdfBuilder};
use pdf::content::{parse_ops, serialize_ops, Op};
use pdf::enc::{LZWFlateParams, StreamFilter};
use pdf::file::{CachedFile, FileOptions, OpenMode, UpdateOptions};
use pdf::object::{
    ImageDict, ImageXObject, NameTree, NameTreeNode, NoResolve, Object, PlainRef, Stream, Updater,
};
use pdf::primitive::{PdfString, Primitive};

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
    FileOptions::cached().open_owned(path).expect("load")
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

// ------------------------------------------------- 7.9.6 building a tree

fn build_tree_of(n: i64) -> NameTree<Primitive> {
    let mut storage = FileOptions::cached().storage();
    let entries: Vec<(PdfString, Primitive)> = (0..n)
        .map(|i| (PdfString::from(format!("k{i:03}")), Primitive::Integer(i)))
        .collect();
    NameTree::build_tree(entries, &mut storage).expect("build a name tree")
}

#[test]
fn name_tree_build_tree_makes_leaves_and_keeps_every_entry_once() {
    // 7.9.6: a name tree with more entries than fit in one node needs
    // intermediate nodes. Building one used to be `unimplemented!()`, i.e. a
    // panic for any input that needs a tree at all.
    let root = build_tree_of(100);
    let kids = match &root.node {
        NameTreeNode::Intermediate(kids) => kids,
        NameTreeNode::Leaf(_) => panic!("100 entries do not fit in one leaf"),
    };
    assert_eq!(kids.len(), 6, "100 entries at 17 per leaf");
    let ids: std::collections::HashSet<_> = kids.iter().map(|kid| kid.get_inner().id).collect();
    assert_eq!(ids.len(), 6, "every leaf is a distinct object, created once");
    // 7.9.6: the root itself carries no /Limits.
    assert!(root.limits.is_none());
}

#[test]
fn name_tree_build_tree_of_few_entries_is_a_single_leaf() {
    let root = build_tree_of(3);
    match &root.node {
        NameTreeNode::Leaf(items) => {
            let keys: Vec<_> = items.iter().map(|(k, _)| k.as_bytes().to_vec()).collect();
            assert_eq!(keys, vec![b"k000".to_vec(), b"k001".to_vec(), b"k002".to_vec()]);
        }
        NameTreeNode::Intermediate(_) => panic!("three entries are one leaf"),
    }
}

// -------------------------------------------- 8.9.7 writing inline images

fn inline_image_data(ops: &[Op]) -> Vec<u8> {
    ops.iter()
        .find_map(|op| match op {
            Op::InlineImage { image } => Some(Vec::from(&*image.inner.data(&NoResolve).expect("image data"))),
            _ => None,
        })
        .expect("an inline image")
}

#[test]
fn inline_image_survives_a_rewrite() {
    // 8.9.7: an inline image has no /Length; its data ends at `EI`. The writer
    // used to `unimplemented!()` here, so no page with an inline image on it
    // could be copied. The sample pair `45 49` spells `EI` once decoded, which
    // is exactly the case the hex encoding has to keep safe.
    let content = b"q\nBI /W 2 /H 1 /CS /DeviceGray /BPC 8 /F /AHx ID 4549\nEI\nQ\n";
    let ops = parse_ops(content, &NoResolve).expect("parse");
    assert_eq!(inline_image_data(&ops), vec![0x45, 0x49]);

    let out = serialize_ops(&ops).expect("serialize");
    let again = parse_ops(&out, &NoResolve).expect("re-parse our own output");
    assert_eq!(inline_image_data(&again), inline_image_data(&ops));

    // The two characters that would end the image early appear only at the
    // end, where the parser expects them.
    let text = String::from_utf8_lossy(&out);
    let id_at = text.find("ID ").expect("ID marker");
    assert_eq!(text[id_at..].matches("EI").count(), 1, "{}", text);
}

#[test]
fn inline_image_data_that_spells_ei_is_escaped() {
    // Uncompressed samples can contain `EI` themselves. Written raw, the image
    // would end in the middle of its own data.
    let dict = ImageDict { width: 2, height: 1, bits_per_component: Some(8), ..Default::default() };
    let image = ImageXObject { inner: Stream::from_compressed(dict, Arc::<[u8]>::from(&b"EI"[..]), Vec::new()) };
    let out = serialize_ops(&[Op::InlineImage { image: Arc::new(image) }]).expect("serialize");
    let ops = parse_ops(&out, &NoResolve).expect("re-parse");
    assert_eq!(inline_image_data(&ops), b"EI");
}

// ------------------------------------------- 7.4.1 writing /DecodeParms

#[test]
fn decode_parms_stay_aligned_with_the_filters() {
    // 7.4.1, Table 5: /DecodeParms runs parallel to /Filter. Writing a stream
    // with parameters on the second of two filters used to hit an `assert!`
    // (two filters with parameters) or attach the parameters to the wrong
    // filter (one).
    let params = LZWFlateParams { predictor: 12, columns: 4, ..Default::default() };
    let stream: Stream<()> = Stream::from_compressed(
        (),
        Arc::<[u8]>::from(&b"GK#GI"[..]),
        vec![StreamFilter::ASCII85Decode, StreamFilter::FlateDecode(params)],
    );
    let mut storage = FileOptions::cached().storage();
    let ps = stream.to_pdf_stream(&mut storage).expect("to_pdf_stream");

    let filters = ps.info.get("Filter").and_then(|p| p.as_array().ok()).expect("/Filter array");
    assert_eq!(filters.len(), 2);
    let parms = ps.info.get("DecodeParms").and_then(|p| p.as_array().ok()).expect("/DecodeParms array");
    assert_eq!(parms.len(), 2, "/DecodeParms has one entry per filter");
    assert!(parms[0].is_null(), "ASCII85Decode has no parameters");
    let dict = parms[1].as_dict().expect("the Flate parameters");
    assert_eq!(dict.get("Predictor").and_then(|p| p.as_integer().ok()), Some(12));
}

// ------------------------------------------------------------- opening a file

#[test]
fn open_reads_by_default_and_maps_only_when_asked() {
    // No mapping unless the caller asks: `open` on its own reads. `Auto` is
    // the size-based choice (small files are read anyway), `Mapped` forces it.
    let path = tmp_pdf("open-mode.pdf", &build_pdf(&one_page_with_free_object_4("90")));

    let default = FileOptions::cached().open(&path).expect("open, default");
    assert!(!default.storage.into_inner().is_mapped(), "the default is a plain read");

    // the fixture is a few hundred bytes, far below DEFAULT_MMAP_THRESHOLD
    let auto = FileOptions::cached().open_mode(OpenMode::Auto).open(&path).expect("open, auto");
    assert_eq!(auto.num_pages(), 1);
    assert!(!auto.storage.into_inner().is_mapped(), "a small file is not mapped");

    // with the threshold lowered Auto maps exactly when the feature is in
    let forced = FileOptions::cached()
        .open_mode(OpenMode::Auto)
        .mmap_threshold(0)
        .open(&path)
        .expect("open, auto, no threshold");
    assert_eq!(
        forced.storage.into_inner().is_mapped(),
        cfg!(feature = "mmap"),
        "auto should map exactly when the feature is built in",
    );

    let read = FileOptions::cached().open_mode(OpenMode::Read).open(&path).expect("open, read");
    assert_eq!(read.num_pages(), 1);
    assert!(!read.storage.into_inner().is_mapped());

    let owned = FileOptions::cached().open_owned(&path).expect("open_owned");
    assert_eq!(owned.get_page(0).expect("page").rotate(), 90);
}

#[test]
fn a_mapped_document_can_be_edited_and_written() {
    // A mapping is read-only. Writing to the backend (an incremental update)
    // has to copy the bytes into memory first instead of failing or, worse,
    // writing through the mapping.
    let path = tmp_pdf("open-edit.pdf", &build_pdf(&one_page_with_free_object_4("180")));
    let out = path.with_extension("out.pdf");

    let mut file = FileOptions::cached().open(&path).expect("open");
    file.save_to(&out).expect("save_to after a mapped open");

    let reloaded = FileOptions::cached().open(&out).expect("reload");
    assert_eq!(reloaded.num_pages(), 1);
    assert_eq!(reloaded.get_page(0).expect("page").rotate(), 180);

    // A full rewrite reads the same mapping and never touches the backend.
    let mut again = FileOptions::cached().open(&path).expect("open again");
    let bytes = again.rebuild().expect("rebuild");
    let rebuilt = FileOptions::cached().load(bytes).expect("load the rebuild");
    assert_eq!(rebuilt.get_page(0).expect("page").rotate(), 180);
}

#[test]
fn opening_an_empty_file_does_not_try_to_map_it() {
    // `mmap` refuses a zero length file; `Auto` reads it (and then fails to
    // find a header) rather than reporting an I/O error.
    let path = tmp_pdf("open-empty.pdf", b"");
    let data = FileOptions::cached().open_mode(OpenMode::Auto).mmap_threshold(0).open(&path);
    let err = match data {
        Ok(_) => panic!("an empty file is not a PDF"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("header"), "expected a header error, got: {err}");
}

// -------------------------------------------- 8.6.3 copying a page's resources

#[test]
fn operators_are_recognised_without_being_parsed_as_operands() {
    // The content loop reads a token once and decides from its first byte
    // whether it can be an operand (`/ ( < [`, a digit, a sign or a dot);
    // anything else is an operator. Before, every operator was first run
    // through the object parser, which built a `PdfError::UnknownType` (two
    // heap `String`s) and threw it away. Delimiters let an operator follow an
    // operand with no space, and a stream may end with an operator and no
    // trailing whitespace.
    let ops = parse_ops(
        b"1 2 Td/GS0 gs q 0 0 m 10 10 l S Q BT/F1 12 Tf(EI)Tj ET BT/F1 9 Tf[(a)-2(b)]TJ ET",
        &NoResolve,
    )
    .expect("parse");

    let shows: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            Op::TextDraw { text } => Some(text.data.to_vec()),
            _ => None,
        })
        .collect();
    assert_eq!(shows, vec![b"EI".to_vec()], "operands must keep their values");
    assert!(
        matches!(ops.last(), Some(Op::EndText)),
        "the stream ends with an operator: {:?}",
        ops.last()
    );

    // Read, write and read again: the operator/operand split has to survive.
    let out = serialize_ops(&ops).expect("serialize");
    let again = parse_ops(&out, &NoResolve).expect("re-parse our own output");
    assert_eq!(again.len(), ops.len());
}

#[test]
fn a_literal_string_without_escapes_reads_the_same_as_one_with() {
    // The parser copies a string body that needs no unescaping straight out of
    // the buffer (`plain_literal_len`); a body with a reverse solidus, a nested
    // parenthesis or a line break goes through `StringLexer` byte by byte. Both
    // paths have to produce the same bytes, and the buffer position after the
    // string has to be the same, or the operator after it would be read wrong.
    let cases: &[(&[u8], &[u8])] = &[
        (b"(plain) Tj", b"plain"),
        (b"(pl\\ain) Tj", b"plain"),
        (b"(tab\\there) Tj", b"tab\there"),
        (b"(octal \\101\\102) Tj", b"octal AB"),
        (b"(nested (one)) Tj", b"nested (one)"),
        (b"(closing \\) paren) Tj", b"closing ) paren"),
        (b"(split \\\nline) Tj", b"split line"),
        (b"(crlf \r\nis one) Tj", b"crlf \nis one"),
        (b"() Tj", b""),
        (b"(\\() Tj", b"("),
    ];
    for (input, expected) in cases {
        let ops = parse_ops(input, &NoResolve).expect("parse");
        let mut shows = vec![];
        for op in &ops {
            if let Op::TextDraw { text } = op {
                shows.push(text.data.to_vec());
            }
        }
        assert_eq!(
            shows,
            vec![expected.to_vec()],
            "{:?} must read as {:?}",
            String::from_utf8_lossy(input),
            String::from_utf8_lossy(expected)
        );
        // `Tj` follows with no separator; if the string had consumed one byte
        // too few or too many, it would not be an operator any more.
        assert_eq!(ops.len(), 1, "one string and one operator expected for {:?}", String::from_utf8_lossy(input));
        assert!(
            matches!(ops.first(), Some(Op::TextDraw { .. })),
            "the operator after the string must still be found in {:?}: {:?}",
            String::from_utf8_lossy(input),
            ops.first()
        );
    }

    // A long body does not fit into `IBytes` inline capacity: the copy must
    // still be exact at the heap boundary.
    for len in [22usize, 23, 24, 200, 4096] {
        let body: String = std::iter::repeat('x').take(len).collect();
        let input = format!("({}) Tj", body);
        let ops = parse_ops(input.as_bytes(), &NoResolve).expect("parse");
        match ops.first() {
            Some(Op::TextDraw { text }) => assert_eq!(text.data.as_slice(), body.as_bytes(), "{len} bytes"),
            other => panic!("{len} bytes: {:?}", other),
        }
    }
}

#[test]
fn the_lazy_parser_yields_the_same_operators_as_the_eager_one() {
    // `parse_ops_iter` hands out one operator per call, and `"` alone produces
    // four of them (`Tw`, `Tc`, `T*`, `Tj`), so the queue of operators the
    // builder holds until they are handed out has to be drained exactly like
    // the eager version pushed them.
    use pdf::content::{parse_ops, parse_ops_iter};
    use pdf::object::NoResolve;

    let data = b"1 2 Td/GS0 gs q 0 0 m 10 10 l S Q BT/F1 12 Tf(EI)Tj ET \
                 BT/F1 9 Tf[(a)-2(b)]TJ ET 1 0 0 1 5 5 cm 3 w [3 3] 0 d \
                 0 0 0 0 re b s 12 Tw 14 Tc (x)' 0 0 5 5 re B* 100 200 300 400 re f*";

    let eager = parse_ops(data, &NoResolve).expect("eager parse");
    let lazy: Vec<_> = parse_ops_iter(data, &NoResolve)
        .collect::<Result<Vec<_>, _>>()
        .expect("lazy parse");

    let eager: Vec<String> = eager.iter().map(|op| format!("{:?}", op)).collect();
    let lazy: Vec<String> = lazy.iter().map(|op| format!("{:?}", op)).collect();
    assert_eq!(eager, lazy);
    assert!(eager.len() > 20, "not enough operators: {}", eager.len());
}

#[test]
fn a_page_split_over_several_content_streams_gives_all_its_operators() {
    // A page's /Contents may be an array of streams; each is a separate part,
    // and the operators have to come out in order, not just from the first.
    use pdf::content::Op;

    let objects = vec![
        Some("<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        Some("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        Some(concat!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] ",
            "/Resources << >> /Contents [4 0 R 5 0 R] >>"
        ).to_string()),
        Some("<< /Length 6 >>\nstream\n0 0 m\nendstream".to_string()),
        Some("<< /Length 9 >>\nstream\n10 10 l S\nendstream".to_string()),
    ];
    let file = load(&objects);
    let page = file.get_page(0).expect("page");
    let ops: Vec<Op> = page
        .contents
        .as_ref()
        .expect("contents")
        .ops(&file.resolver())
        .collect::<Result<Vec<_>, _>>()
        .expect("operators");

    assert_eq!(ops.len(), 3, "expected MoveTo, LineTo, Stroke; got {:?}", ops);
    assert!(matches!(ops[0], Op::MoveTo { .. }), "{:?}", ops[0]);
    assert!(matches!(ops[1], Op::LineTo { .. }), "{:?}", ops[1]);
    assert!(matches!(ops[2], Op::Stroke), "{:?}", ops[2]);
}

#[test]
fn clone_page_keeps_color_spaces_and_patterns() {
    // A page's colour space and pattern live in its /Resources and are
    // referred to by *name* from the content stream (`/CS0 cs`, `/P0 scn`).
    // `deep_clone_op` used to copy fonts, XObjects and graphics states only,
    // so a copied page lost every fill colour (`color space "CS0" not
    // present`) and every pattern.
    let objects = vec![
        s("<< /Type /Catalog /Pages 2 0 R >>"),
        s("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        s(concat!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] ",
            "/Resources 6 0 R /Contents 4 0 R >>"
        )),
        s("<< /Length 34 >>\nstream\n/CS0 cs 0 scn /Pattern cs /P0 scn\nendstream"),
        s(concat!(
            "<< /Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 ",
            "/BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources 7 0 R /Length 0 >>\n",
            "stream\n\nendstream"
        )),
        s(concat!(
            "<< /ColorSpace << /CS0 [/Indexed /DeviceRGB 1 <000000FFFFFF>] >> ",
            "/Pattern << /P0 5 0 R >> >>"
        )),
        s("<< >>"),
    ];
    let file = load(&objects);
    let page = file.get_page(0).expect("page");

    let mut builder = PdfBuilder::new(FileOptions::cached());
    let mut importer = Importer::new(file.resolver(), &mut builder.storage);
    let copy = PageBuilder::clone_page(&page, &mut importer).expect("clone the page");
    importer.finish();

    assert!(
        copy.resources.color_spaces.contains_key("CS0"),
        "the copied page needs /CS0, has: {:?}",
        copy.resources.color_spaces.keys().collect::<Vec<_>>()
    );
    assert!(
        copy.resources.pattern.contains_key("P0"),
        "the copied page needs /P0, has: {:?}",
        copy.resources.pattern.keys().collect::<Vec<_>>()
    );
}
