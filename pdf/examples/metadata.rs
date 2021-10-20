use std::env::args;

use pdf::error::PdfError;
use pdf::file::File;

/// extract and print a PDF's metadata
fn main() -> Result<(), PdfError> {
    let path = args()
        .nth(1)
        .expect("Please provide a file path to the PDF you want to explore.");

    let file = File::<Vec<u8>>::open(&path).unwrap();
    if let Some(ref info) = file.trailer.info_dict {
        info.iter()
            .filter(|(_, primitive)| primitive.as_str().is_some())
            .for_each(|(key, value)| {
                eprintln!("{:>15}: {}", key, value.as_str().unwrap());
            });
    }

    Ok(())
}
