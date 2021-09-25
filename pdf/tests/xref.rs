#[test]
fn infinite_loop_invalid_file() {
    assert!(pdf::file::File::from_data(b"startxref%PDF-".as_ref()).is_err());
}
