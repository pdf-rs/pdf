use pdf::file::FileOptions;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
fn rebuild_file() {
    let pdf = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../files/283-rebuild-crash.pdf")).unwrap();
    let mut file = FileOptions::uncached().load(pdf).unwrap();  // loads fine
    // This panics:
    file.rebuild().unwrap();
}
