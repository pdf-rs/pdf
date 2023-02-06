use pdf::file::FileOptions;

#[test]
fn infinite_loop_invalid_file() {
    assert!(FileOptions::uncached().load(b"startxref%PDF-".as_ref()).is_err());
}

#[test]
fn ending_angle_bracket() {
    assert!(FileOptions::uncached().load(b"%PDF-startxref>".as_ref()).is_err());
    assert!(FileOptions::uncached().load(b"%PDF-startxref<".as_ref()).is_err());
}
