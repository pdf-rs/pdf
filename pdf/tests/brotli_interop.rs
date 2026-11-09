use pdf::enc::{decode, LZWFlateParams, StreamFilter};
use pdf::file::FileOptions;
use pdf::object::ParseOptions;

#[test]
fn brotli_predictors_limits_and_large_window() {
    let options = ParseOptions::strict();
    let expected = [100, 3, 101, 2, 102, 1];
    for (predictor, encoded) in [
        (
            12,
            b"\x0b\x04\x80\x02\x64\x03\x02\x01\xff\x02\x01\xff\x03".as_slice(),
        ),
        (2, b"\x8b\x02\x80\x64\x9f\x65\x9d\x66\x9b\x03".as_slice()),
    ] {
        let params = LZWFlateParams {
            predictor,
            columns: 2,
            ..Default::default()
        };
        assert_eq!(
            decode(encoded, &StreamFilter::BrotliDecode(params), &options).unwrap(),
            expected
        );
    }
    let large = b"\x11\x19\x22\x00\x02Brotli PDF stream\n\x03";
    let filter = StreamFilter::BrotliDecode(Default::default());
    assert_eq!(
        decode(large, &filter, &options).unwrap(),
        b"Brotli PDF stream\n"
    );
    let mut limited = options.clone();
    limited.object_size_limit = 4;
    assert!(decode(large, &filter, &limited).is_err());
    for len in 0..large.len() {
        assert!(decode(&large[..len], &filter, &options).is_err());
    }
}

#[test]
fn predicted_brotli_xref_opens() {
    let file = FileOptions::cached()
        .load(include_bytes!("resources/brotli/png-xref.pdf").to_vec())
        .unwrap();
    assert_eq!(file.num_pages(), 1);
    assert!(file.get_page(0).is_ok());
}

#[test]
fn corpus_content_fonts_and_images() {
    use pdf::object::{Resolve, XObject};
    let expected: Vec<u8> = (0..8)
        .flat_map(|y| (0..8).flat_map(move |x| [x * 32, y * 32, 128]))
        .collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/resources/brotli");
    let mut count = 0;
    for suite in ["brotli", "brotli-large-window"] {
        for entry in std::fs::read_dir(root.join(suite)).unwrap() {
            let path = entry.unwrap().path();
            let file = FileOptions::cached()
                .load(std::fs::read(&path).unwrap())
                .unwrap();
            let resolver = file.resolver();
            assert_eq!(file.num_pages(), 1);
            let page = file.get_page(0).unwrap();
            assert!(!page
                .contents
                .as_ref()
                .unwrap()
                .operations(&resolver)
                .unwrap()
                .is_empty());
            let resources = page.resources().unwrap();
            assert!(resources
                .fonts
                .values()
                .next()
                .unwrap()
                .load(&resolver)
                .is_ok());
            let object = resolver
                .get(*resources.xobjects.values().next().unwrap())
                .unwrap();
            if let XObject::Image(image) = &*object {
                assert_eq!(
                    image.image_data(&resolver).unwrap().as_ref(),
                    expected.as_slice()
                );
            } else {
                panic!("missing image: {:?}", path);
            }
            count += 1;
        }
    }
    assert_eq!(count, 12);
}

#[test]
fn tiff_sample_depths_and_row_padding() {
    {
        let params = LZWFlateParams {
            predictor: 2,
            columns: 5,
            n_components: 1,
            bits_per_component: 1,
            ..Default::default()
        };
        assert_eq!(
            decode(
                &[143, 0, 128, 232, 112, 3],
                &StreamFilter::BrotliDecode(params),
                &ParseOptions::strict()
            )
            .unwrap(),
            [176, 88]
        );
    }
    {
        let params = LZWFlateParams {
            predictor: 2,
            columns: 5,
            n_components: 1,
            bits_per_component: 2,
            ..Default::default()
        };
        assert_eq!(
            decode(
                &[143, 0, 128, 219, 0, 3],
                &StreamFilter::BrotliDecode(params),
                &ParseOptions::strict()
            )
            .unwrap(),
            [201, 64]
        );
    }
    {
        let params = LZWFlateParams {
            predictor: 2,
            columns: 5,
            n_components: 1,
            bits_per_component: 4,
            ..Default::default()
        };
        assert_eq!(
            decode(
                &[15, 1, 128, 243, 255, 208, 3],
                &StreamFilter::BrotliDecode(params),
                &ParseOptions::strict()
            )
            .unwrap(),
            [242, 16, 208]
        );
    }
    {
        let params = LZWFlateParams {
            predictor: 2,
            columns: 2,
            n_components: 1,
            bits_per_component: 8,
            ..Default::default()
        };
        assert_eq!(
            decode(
                &[143, 2, 128, 100, 159, 101, 157, 102, 155, 3],
                &StreamFilter::BrotliDecode(params),
                &ParseOptions::strict()
            )
            .unwrap(),
            [100, 3, 101, 2, 102, 1]
        );
    }
    {
        let params = LZWFlateParams {
            predictor: 2,
            columns: 2,
            n_components: 1,
            bits_per_component: 16,
            ..Default::default()
        };
        assert_eq!(
            decode(
                &[143, 3, 128, 255, 250, 0, 10, 128, 0, 128, 0, 3],
                &StreamFilter::BrotliDecode(params),
                &ParseOptions::strict()
            )
            .unwrap(),
            [255, 250, 0, 4, 128, 0, 0, 0]
        );
    }
    {
        let params = LZWFlateParams {
            predictor: 2,
            columns: 2,
            n_components: 3,
            bits_per_component: 8,
            ..Default::default()
        };
        assert_eq!(
            decode(
                &[143, 2, 128, 250, 30, 100, 10, 1, 246, 3],
                &StreamFilter::BrotliDecode(params),
                &ParseOptions::strict()
            )
            .unwrap(),
            [250, 30, 100, 4, 31, 90]
        );
    }
}

#[test]
fn minimal_document_decodes_end_to_end() {
    // A whole document, 1 kB, in which every compressed thing is Brotli: both
    // content streams, an image with a PNG predictor, the object stream that
    // holds the catalog and the page tree, and the cross-reference stream. A
    // reader without /BrotliDecode finds no page at all, so this one file
    // covers the read path end to end instead of filter by filter.
    // Generated by tests/resources/brotli/minimal/make.mjs (node, no deps).
    use pdf::object::{Resolve, XObject};
    let file = FileOptions::cached()
        .load(include_bytes!("resources/brotli/minimal/brotli-minimal.pdf").to_vec())
        .unwrap();
    let resolver = file.resolver();
    assert_eq!(file.num_pages(), 2);

    let page = file.get_page(0).unwrap();
    let resources = page.resources().unwrap();
    assert_eq!(resources.fonts.len(), 1);
    for r in resources.fonts.values() {
        assert!(r.load(&resolver).is_ok());
    }
    assert_eq!(
        page.contents
            .as_ref()
            .unwrap()
            .operations(&resolver)
            .unwrap()
            .len(),
        9
    );

    let page = file.get_page(1).unwrap();
    let resources = page.resources().unwrap();
    assert_eq!(
        page.contents
            .as_ref()
            .unwrap()
            .operations(&resolver)
            .unwrap()
            .len(),
        4
    );
    let object = resolver
        .get(*resources.xobjects.values().next().unwrap())
        .unwrap();
    let XObject::Image(image) = &*object else {
        panic!("not an image");
    };
    assert_eq!((image.width, image.height), (64, 64));
    let expected: Vec<u8> = (0..64u32)
        .flat_map(|y| (0..64u32).flat_map(move |x| [(x * 4) as u8, (y * 4) as u8, 0x40]))
        .collect();
    assert_eq!(
        image.image_data(&resolver).unwrap().as_ref(),
        expected.as_slice()
    );
}
