#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(p) = pdf::file::File::from_data(data) {
        for _ in p.pages() {}
    }
});
