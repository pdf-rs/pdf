#![no_main]
use libfuzzer_sys::fuzz_target;
use pdf::enc::*;
use pdf::object::ParseOptions;
use std::convert::TryInto;

fn get_i32(data: &[u8], offset: &mut usize) -> i32 {
    if *offset + 4 > data.len() {
        return 0;
    }
    let val = i32::from_le_bytes(data[*offset..*offset+4].try_into().unwrap());
    *offset += 4;
    val
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 1 { return; }
    let mut offset = 0;

    let filter_type = data[offset] % 6;
    offset += 1;

    let filter = match filter_type {
        0 => StreamFilter::ASCIIHexDecode,
        1 => StreamFilter::ASCII85Decode,
        2 | 3 => {
            let predictor = get_i32(data, &mut offset);
            let n_components = get_i32(data, &mut offset);
            let bits_per_component = get_i32(data, &mut offset);
            let columns = get_i32(data, &mut offset);
            let early_change = get_i32(data, &mut offset);

            let params = LZWFlateParams {
                predictor,
                n_components,
                bits_per_component,
                columns,
                early_change
            };

            if filter_type == 2 {
                StreamFilter::LZWDecode(params)
            } else {
                StreamFilter::FlateDecode(params)
            }
        },
        4 => StreamFilter::RunLengthDecode,
        5 => {
             // DCT
             let color_transform = Some(get_i32(data, &mut offset));
             StreamFilter::DCTDecode(DCTDecodeParams { color_transform })
        },
        _ => return
    };

    if offset > data.len() { return; }
    let payload = &data[offset..];

    let options = ParseOptions::tolerant();
    // Execute decode
    let _ = decode(payload, &filter, &options);
});
