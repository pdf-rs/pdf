use itertools::Itertools;
use tuple::*;
use types::StreamFilter;
use inflate::InflateStream;
use err::*;
use std::mem;
use std::str;

use primitive::Dictionary;

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


fn flate_decode(data: &[u8], params: &Option<Dictionary>) -> Result<Vec<u8>> {
    println!("DEBUG PARAMS {:?}", params.as_ref().unwrap());
    println!("DEBUG DATA(len: {}): {:?}", data.len(), data);
    // TODO: write macro
    let predictor = match *params {
        Some(ref params) => params.get("Predictor").map_or(Ok(1), |x| x.as_integer())?,
        None => 1,
    };
    let n_components = match *params {
        Some(ref params) => params.get("Colors").map_or(Ok(1), |x| x.as_integer())?,
        None => 1,
    } as usize;
    let bits_per_component = match *params {
        Some(ref params) => params.get("BitsPerComponentt").map_or(Ok(8), |x| x.as_integer())?,
        None => 8,
    };
    let columns = match *params {
        Some(ref params) => params.get("Columns").map_or(Ok(1), |x| x.as_integer())?,
        None => 1,
    } as usize;
    //

    // First flate decode
    let mut inflater = InflateStream::from_zlib();
    let mut out = Vec::<u8>::new();
    let mut n = 0;
    while n < data.len() {
        let res = inflater.update(&data[n..]);
        let (num_bytes_read, result) = res?;
        n += num_bytes_read;
        out.extend(result);
    }

    // Then unfilter (PNG)
    // For this, take the old out as input, and write output to out

    if predictor > 10 {
        let inp = out; // input buffer
        let rows = inp.len() / (columns+1);
        
        // output buffer
        let mut out = vec![0; rows * columns];
    
        // Apply inverse predictor
        let null_vec = vec![0; columns];
        
        let mut in_off = 0; // offset into input buffer
        
        let mut out_off = 0; // offset into output buffer
        let mut last_out_off = 0; // last offset to output buffer
        
        while in_off < inp.len() {
            
            let predictor = PredictorType::from_u8(inp[in_off])?;
            in_off += 1; // +1 because the first byte on each row is predictor
            
            let row_in = &inp[in_off .. in_off + columns];
            let (prev_row, row_out) = if last_out_off == 0 {
                (&null_vec[..], &mut out[out_off .. out_off+columns])
            } else {
                let (prev, curr) = out.split_at_mut(out_off);
                (&prev[last_out_off ..], &mut curr[.. columns])
            };
            // println!("{:?} {:?}", predictor, row_in);
            unfilter(predictor, n_components, prev_row, row_in, row_out);
            // println!("-> {:?}", row_out);
            
            last_out_off = out_off;
            
            in_off += columns;
            out_off += columns;
        }
        Ok(out)
    } else {
        Ok(out)
    }
}


pub fn decode(data: &[u8], filter: StreamFilter, params: &Option<Dictionary>) -> Result<Vec<u8>> {
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
#[allow(dead_code)]
pub enum PredictorType {
    NoFilter = 0,
    Sub = 1,
    Up = 2,
    Avg = 3,
    Paeth = 4
}

impl PredictorType {  
    /// u8 -> Self. Temporary solution until Rust provides a canonical one.
    pub fn from_u8(n: u8) -> Result<PredictorType> {
        match n {
            n if n <= 4 => Ok(unsafe { mem::transmute(n) }),
            n => Err(ErrorKind::IncorrectPredictorType {n}.into())
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

pub fn unfilter(filter: PredictorType, bpp: usize, prev: &[u8], inp: &[u8], out: &mut [u8]) {
    use self::PredictorType::*;
    let len = inp.len();
    assert_eq!(len, out.len());
    assert_eq!(len, prev.len());

    match filter {
        NoFilter => {
            for i in 0..len {
                out[i] = inp[i];
            }
        }
        Sub => {
            for i in bpp..len {
                out[i] = inp[i].wrapping_add(
                    out[i - bpp]
                );
            }
        }
        Up => {
            for i in 0..len {
                out[i] = inp[i].wrapping_add(
                    prev[i]
                );
            }
        }
        Avg => {
            for i in 0..bpp {
                out[i] = inp[i].wrapping_add(
                    prev[i] / 2
                );
            }

            for i in bpp..len {
                out[i] = inp[i].wrapping_add(
                    ((out[i - bpp] as i16 + prev[i] as i16) / 2) as u8
                );
            }
        }
        Paeth => {
            for i in 0..bpp {
                out[i] = inp[i].wrapping_add(
                    filter_paeth(0, prev[i], 0)
                );
            }

            for i in bpp..len {
                out[i] = inp[i].wrapping_add(
                    filter_paeth(out[i - bpp], prev[i], prev[i - bpp])
                );
            }
        }
    }
}

pub fn filter(method: PredictorType, bpp: usize, previous: &[u8], current: &mut [u8]) {
    use self::PredictorType::*;
    let len  = current.len();

    match method {
        NoFilter => (),
        Sub => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(current[i - bpp]);
            }
        }
        Up => {
            for i in 0..len {
                current[i] = current[i].wrapping_sub(previous[i]);
            }
        }
        Avg => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub((current[i - bpp].wrapping_add(previous[i]) / 2));
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(previous[i] / 2);
            }
        }
        Paeth => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(filter_paeth(current[i - bpp], previous[i], previous[i - bpp]));
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(filter_paeth(0, previous[i], 0));
            }
        }
    }
}
