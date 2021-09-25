#[test]
fn infinite_loop_invalid_file() {
    assert!(pdf::file::File::from_data(b"startxref%PDF-".as_ref()).is_err());
}

#[test]
fn ending_angle_bracket() {
    assert!(pdf::file::File::from_data(b"%PDF-startxref>".as_ref()).is_err());
    assert!(pdf::file::File::from_data(b"%PDF-startxref<".as_ref()).is_err());
}
