#![no_main]
use libfuzzer_sys::fuzz_target;

fn harness(data: &[u8]) {
    if let Ok(file) = pdf::file::FileOptions::cached().load(data) {
        for idx in 0..file.num_pages() {
            let _ = file.get_page(idx);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let _ = harness(data);
});