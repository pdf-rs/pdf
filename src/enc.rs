use itertools::Itertools;
use tuple::*;
use types::StreamFilter;
use std::convert::TryFrom;
use inflate::InflateStream;
use err::*;
use stream::DecodeParams;


fn decode_nibble(c: u8) -> Option<u8> {
    match c {
        n @ b'0' ... b'9' => Some(n - b'0'),
        a @ b'a' ... b'h' => Some(a - b'a' + 0xa),
        a @ b'A' ... b'H' => Some(a - b'A' + 0xA),
        _ => None
    }
}

fn decode_hex(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 2);
    for (i, (&high, &low)) in data.iter().tuples().enumerate() {
        if let (Some(low), Some(high)) = (decode_nibble(low), decode_nibble(high)) {
            out.push(high << 4 | low);
        } else {
            return Err(ErrorKind::HexDecode {pos: i * 2, bytes: [high, low]}.into())
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

fn decode_85(data: &[u8]) -> Result<Vec<u8>> {
    use std::iter::repeat;
    
    let mut out = Vec::with_capacity(data.len());
    
    let mut pos = 0;
    while let Some((advance, word)) = word_85(&data[pos..]) {
        out.extend_from_slice(&word);
        pos += advance as usize;
    }
    let tail_len = substr(&data[pos..], b"~>").ok_or(ErrorKind::Ascii85TailError)?;
    assert!(tail_len < 5);
    let tail: [u8; 5] = T5::from_iter(
        data[pos..pos+tail_len].iter()
        .cloned()
        .chain(repeat(b'u'))
    )
    .ok_or(ErrorKind::Ascii85TailError)?
    .into();
    
    let (_, last) = word_85(&tail).ok_or(ErrorKind::Ascii85TailError)?;
    out.extend_from_slice(&last[.. tail_len-1]);
    Ok(out)
}

fn flate_decode(data: &[u8], params: &DecodeParams) -> Result<Vec<u8>> {
    let mut inflater = InflateStream::from_zlib();
    let mut out = Vec::<u8>::new();
    let mut n = 0;
    while n < data.len() {
        let res = inflater.update(&data[n..]);
        let (num_bytes_read, result) = res?;
        n += num_bytes_read;
        out.extend(result);
    }

    // TODO NOW
    // Next up: provide default params? for when calling decode()
    if params.predictor > 10 {
        // Apply inverse predictor
        let i = 0;
        let null_vec = vec![0; params.columns];
        let prev_row = &null_vec;
        while i < out.len() {
            // +1 because the first byte on each row is predictor
            let predictor_nr = out[i];
            let row = &mut out[(i+1)..(i+params.columns)];
            unfilter(PredictorType::from_u8(predictor_nr), params.n_components, &previous, row);
            i += params.columns;
            prev_row = &row;
        }
    }
    Ok(out)
}


pub fn decode(data: &[u8], filter: StreamFilter, params: &DecodeParams) -> Result<Vec<u8>> {
    use self::StreamFilter::*;
    match filter {
        AsciiHex => decode_hex(data),
        Ascii85 => decode_85(data),
        Lzw => unimplemented!(),
        Flate => flate_decode(data, params),
        Jpeg2k => unimplemented!()
    }
}


/*
 * Predictor - copied and adapted from PNG crate..
 */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PredictorType {
    NoFilter = 0,
    Sub = 1,
    Up = 2,
    Avg = 3,
    Paeth = 4
}

 impl PredictorType {  
    /// u8 -> Self. Temporary solution until Rust provides a canonical one.
    pub fn from_u8(n: u8) -> Option<PredictorType> {
        match n {
            n if n <= 4 => Some(unsafe { mem::transmute(n) }),
            _ => None
        }
    }
}

fn filter_paeth(a: u8, b: u8, c: u8) -> u8 {
    let ia = a as i16;
    let ib = b as i16;
    let ic = c as i16;

    let p = ia + ib - ic;

    let pa = (p - ia).abs();
    let pb = (p - ib).abs();
    let pc = (p - ic).abs();

    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

pub fn unfilter(filter: PredictorType, bpp: usize, previous: &[u8], current: &mut [u8]) {
    let len = current.len();

    match filter {
        NoFilter => (),
        Sub => {
            for i in bpp..len {
                current[i] = current[i].wrapping_add(
                    current[i - bpp]
                );
            }
        }
        Up => {
            for i in 0..len {
                current[i] = current[i].wrapping_add(
                    previous[i]
                );
            }
        }
        Avg => {
            for i in 0..bpp {
                current[i] = current[i].wrapping_add(
                    previous[i] / 2
                );
            }

            for i in bpp..len {
                current[i] = current[i].wrapping_add(
                    ((current[i - bpp] as i16 + previous[i] as i16) / 2) as u8
                );
            }
        }
        Paeth => {
            for i in 0..bpp {
                current[i] = current[i].wrapping_add(
                    filter_paeth(0, previous[i], 0)
                );
            }

            for i in bpp..len {
                current[i] = current[i].wrapping_add(
                    filter_paeth(current[i - bpp], previous[i], previous[i - bpp])
                );
            }
        }
    }
}

pub fn filter(method: PredictorType, bpp: usize, previous: &[u8], current: &mut [u8]) {
    let len  = current.len();

    match method {
        NoFilter => (),
        Sub      => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(current[i - bpp]);
            }
        }
        Up       => {
            for i in 0..len {
                current[i] = current[i].wrapping_sub(previous[i]);
            }
        }
        Avg  => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub((current[i - bpp].wrapping_add(previous[i]) / 2));
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(previous[i] / 2);
            }
        }
        Paeth    => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(filter_paeth(current[i - bpp], previous[i], previous[i - bpp]));
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(filter_paeth(0, previous[i], 0));
            }
        }
    }
}
