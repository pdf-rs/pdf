use itertools::Itertools;
use tuple::*;
use types::StreamFilter;
use std::convert::TryFrom;

pub enum DecodeError {
    HexDecode(usize, [u8; 2]),
    Ascii85TailError
}
fn decode_nibble(c: u8) -> Option<u8> {
    match c {
        n @ b'0' ... b'9' => Some(n - b'0'),
        a @ b'a' ... b'h' => Some(a - b'a' + 0xa),
        a @ b'A' ... b'H' => Some(a - b'A' + 0xA),
        _ => None
    }
}

fn decode_hex(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(data.len() / 2);
    for (i, (&high, &low)) in data.iter().tuples().enumerate() {
        if let (Some(low), Some(high)) = (decode_nibble(low), decode_nibble(high)) {
            out.push(high << 4 | low);
        } else {
            return Err(DecodeError::HexDecode(i * 2, [high, low]))
        }
    }
    Ok(out)
}

#[inline]
fn sym_85(byte: u8) -> Option<u8> {
    match byte {
        b @ 0x21 ... 0x75 => Some(b - 0x21),
        _ => None
    }
}
fn word_85(input: &[u8]) -> Option<(u8, [u8; 4])> {
    match input.get(0).cloned() {
        Some(b'z') => Some((1, [0; 4])),
        Some(a) => T4::from_iter(input[1 .. 5].iter().cloned()).and_then(|t| {
            T1(a).join(t)
            .map(sym_85).collect()
            .map(|v| v.map(|x| x as u32))
            .map(|T5(a, b, c, d, e)| {
                let q: u32 = ((((a * 85) + b * 85) + c * 85) + d * 85) + e;
                (5, [(q >> 24) as u8, (q >> 16) as u8, (q >> 8) as u8, q as u8])
            })
        }),
        None => None
    }
}

fn substr(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

fn decode_85(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
    use std::iter::repeat;
    
    let mut out = Vec::with_capacity(data.len());
    
    let mut pos = 0;
    while let Some((advance, word)) = word_85(&data[pos..]) {
        out.extend_from_slice(&word);
        pos += advance as usize;
    }
    let tail_len = substr(&data[pos..], b"~>").ok_or(DecodeError::Ascii85TailError)?;
    assert!(tail_len < 5);
    let tail: [u8; 5] = T5::from_iter(
        data[pos..pos+tail_len].iter()
        .cloned()
        .chain(repeat(b'u'))
    )
    .ok_or(DecodeError::Ascii85TailError)?
    .into();
    
    let (_, last) = word_85(&tail).ok_or(DecodeError::Ascii85TailError)?;
    out.extend_from_slice(&last[.. tail_len-1]);
    Ok(out)
}


fn decode(data: &[u8], filter: StreamFilter) -> Result<Vec<u8>, DecodeError> {
    use self::StreamFilter::*;
    match filter {
        AsciiHex => decode_hex(data),
        Ascii85 => decode_85(data),
        Lzw => unimplemented!(),
        Flate => unimplemented!(),
        Jpeg2k => unimplemented!()
    }
}

