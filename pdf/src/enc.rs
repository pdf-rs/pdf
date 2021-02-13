use itertools::Itertools;
use inflate::{inflate_bytes_zlib, inflate_bytes};
use deflate::deflate_bytes;

use crate as pdf;
use crate::error::*;
use crate::object::{Object, Resolve};
use crate::primitive::{Primitive, Dictionary};
use std::convert::TryInto;


#[derive(Object, ObjectWrite, Debug, Clone)]
pub struct LZWFlateParams {
    #[pdf(key="Predictor", default="1")]
    predictor: i32,
    #[pdf(key="Colors", default="1")]
    n_components: i32,
    #[pdf(key="BitsPerComponent", default="8")]
    bits_per_component: i32,
    #[pdf(key="Columns", default="1")]
    columns: i32,
    #[pdf(key="EarlyChange", default="1")]
    early_change: i32,
}
impl Default for LZWFlateParams {
    fn default() -> LZWFlateParams {
        LZWFlateParams {
            predictor: 1,
            n_components: 1,
            bits_per_component: 8,
            columns: 1,
            early_change: 1
        }
    }
}

#[derive(Object, ObjectWrite, Debug, Clone)]
pub struct DCTDecodeParams {
    // TODO The default value of ColorTransform is 1 if the image has three components and 0 otherwise.
    // 0:   No transformation.
    // 1:   If the image has three color components, transform RGB values to YUV before encoding and from YUV to RGB after decoding.
    //      If the image has four components, transform CMYK values to YUVK before encoding and from YUVK to CMYK after decoding.
    //      This option is ignored if the image has one or two color components.
    #[pdf(key="ColorTransform")]
    color_transform: Option<i32>,
}

#[derive(Debug, Clone)]
pub enum StreamFilter {
    ASCIIHexDecode,
    ASCII85Decode,
    LZWDecode (LZWFlateParams),
    FlateDecode (LZWFlateParams),
    JPXDecode, //Jpeg2k
    DCTDecode (DCTDecodeParams),
    CCITTFaxDecode,
    Crypt
}
impl StreamFilter {
    pub fn from_kind_and_params(kind: &str, params: Dictionary, r: &impl Resolve) -> Result<StreamFilter> {
       let params = Primitive::Dictionary (params);
       Ok(
       match kind {
           "ASCIIHexDecode" => StreamFilter::ASCIIHexDecode,
           "ASCII85Decode" => StreamFilter::ASCII85Decode,
           "LZWDecode" => StreamFilter::LZWDecode (LZWFlateParams::from_primitive(params, r)?),
           "FlateDecode" => StreamFilter::FlateDecode (LZWFlateParams::from_primitive(params, r)?),
           "JPXDecode" => StreamFilter::JPXDecode,
           "DCTDecode" => StreamFilter::DCTDecode (DCTDecodeParams::from_primitive(params, r)?),
           "CCITTFaxDecode" => StreamFilter::CCITTFaxDecode,
           "Crypt" => StreamFilter::Crypt,
           ty => bail!("Unrecognized filter type {:?}", ty),
       } 
       )
    }
}

#[inline]
fn decode_nibble(c: u8) -> Option<u8> {
    match c {
        n @ b'0' ..= b'9' => Some(n - b'0'),
        a @ b'a' ..= b'h' => Some(a - b'a' + 0xa),
        a @ b'A' ..= b'H' => Some(a - b'A' + 0xA),
        _ => None
    }
}

#[inline]
fn encode_nibble(c: u8) -> u8 {
    match c {
        0 ..= 9 => b'0'+ c,
        10 ..= 15 => b'a'+ c,
        _ => unreachable!()
    }
}


pub fn decode_hex(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 2);
    for (i, (&high, &low)) in data.iter().tuples().enumerate() {
        if let (Some(low), Some(high)) = (decode_nibble(low), decode_nibble(high)) {
            out.push(high << 4 | low);
        } else {
            return Err(PdfError::HexDecode {pos: i * 2, bytes: [high, low]})
        }
    }
    Ok(out)
}
pub fn encode_hex(data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(data.len() * 2);
    for &b in data {
        buf.push(encode_nibble(b >> 4));
        buf.push(encode_nibble(b & 0xf));
    }
    buf
}

#[inline]
fn sym_85(byte: u8) -> Option<u8> {
    match byte {
        b @ 0x21 ..= 0x75 => Some(b - 0x21),
        _ => None
    }
}

fn word_85([a, b, c, d, e]: [u8; 5]) -> Option<[u8; 4]> {
    fn s(b: u8) -> Option<u32> { sym_85(b).map(|n| n as u32) }
    let (a, b, c, d, e) = (s(a)?, s(b)?, s(c)?, s(d)?, s(e)?);
    let q = (((a * 85 + b) * 85 + c) * 85 + d) * 85 + e;
    Some(q.to_be_bytes())
}

fn decode_85(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity((data.len() + 4) / 5 * 4);
    
    let mut stream = data.iter().cloned()
        .filter(|&b| !matches!(b, b' ' | b'\n' | b'\r' | b'\t'));

    let mut symbols = stream.by_ref()
        .take_while(|&b| b != b'~');

    let (tail_len, tail) = loop {
        match symbols.next() {
            Some(b'z') => out.extend_from_slice(&[0; 4]),
            Some(a) => {
                let (b, c, d, e) = match (symbols.next(), symbols.next(), symbols.next(), symbols.next()) {
                    (Some(b), Some(c), Some(d), Some(e)) => (b, c, d, e),
                    (None, _, _, _) => break (1, [a, b'u', b'u', b'u', b'u']),
                    (Some(b), None, _, _) => break (2, [a, b, b'u', b'u', b'u']),
                    (Some(b), Some(c), None, _) => break (3, [a, b, c, b'u', b'u']),
                    (Some(b), Some(c), Some(d), None) => break (4, [a, b, c, d, b'u']),
                };
                out.extend_from_slice(&word_85([a, b, c, d, e]).ok_or(PdfError::Ascii85TailError)?);
            }
            None => break (0, [b'u'; 5])
        }
    };

    if tail_len > 0 {
        let last = word_85(tail).ok_or(PdfError::Ascii85TailError)?;
        out.extend_from_slice(&last[.. tail_len-1]);
    }

    match (stream.next(), stream.next()) {
        (Some(b'>'), None) => Ok(out),
        _ => Err(PdfError::Ascii85TailError)
    }
}

#[inline]
fn divmod(n: u32, m: u32) -> (u32, u32) {
    (n / m, n % m)
}

#[inline]
fn a85(n: u32) -> u8 {
    n as u8 + 0x21
}

#[inline]
fn base85_chunk(c: [u8; 4]) -> [u8; 5] {
    let n = u32::from_be_bytes(c);
    let (n, e) = divmod(n, 85);
    let (n, d) = divmod(n, 85);
    let (n, c) = divmod(n, 85);
    let (a, b) = divmod(n, 85);
    
    [a85(a), a85(b), a85(c), a85(d), a85(e)]
}

fn encode_85(data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity((data.len() / 4) * 5 + 10);
    let mut chunks = data.chunks_exact(4);
    for chunk in chunks.by_ref() {
        let c: [u8; 4] = chunk.try_into().unwrap();
        if c == [0; 4] {
            buf.push(b'z');
        } else {
            buf.extend_from_slice(&base85_chunk(c));
        }
    }

    let r = chunks.remainder();
    if r.len() > 0 {
        let mut c = [0; 4];
        c[.. r.len()].copy_from_slice(r);
        let out = base85_chunk(c);
        buf.extend_from_slice(&out[.. r.len() + 1]);
    }
    buf.extend_from_slice(b"~>");
    buf
}

#[test]
fn base_85() {
    fn s(b: &[u8]) -> &str { std::str::from_utf8(b).unwrap() }

    let case = &b"hello world!"[..];
    let encoded = encode_85(case);
    assert_eq!(s(&encoded), "BOu!rD]j7BEbo80~>");
    let decoded = decode_85(&encoded).unwrap();
    assert_eq!(case, &*decoded);

    assert_eq!(
        s(&decode_85(
            &lzw_decode(
                &decode_85(&include_bytes!("data/t01_lzw+base85.txt")[..]).unwrap(),
                &LZWFlateParams::default()
            ).unwrap()
        ).unwrap()),
        include_str!("data/t01_plain.txt")
    );
}


fn flate_decode(data: &[u8], params: &LZWFlateParams) -> Result<Vec<u8>> {
    let predictor = params.predictor as usize;
    let n_components = params.n_components as usize;
    let columns = params.columns as usize;

    // First flate decode
    let decoded = match inflate_bytes_zlib(data) {
        Ok(data) => data,
        Err(_) => {
            info!("invalid zlib header. trying without");
            inflate_bytes(data)?
        }
    };
    // Then unfilter (PNG)
    // For this, take the old out as input, and write output to out

    if predictor > 10 {
        let inp = decoded; // input buffer
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
            let (prev_row, row_out) = if out_off == 0 {
                (&null_vec[..], &mut out[out_off .. out_off+columns])
            } else {
                let (prev, curr) = out.split_at_mut(out_off);
                (&prev[last_out_off ..], &mut curr[.. columns])
            };
            unfilter(predictor, n_components, prev_row, row_in, row_out);
            
            last_out_off = out_off;
            
            in_off += columns;
            out_off += columns;
        }
        Ok(out)
    } else {
        Ok(decoded)
    }
}
fn flate_encode(data: &[u8]) -> Vec<u8> {
    deflate_bytes(data)
}

fn dct_decode(data: &[u8], _params: &DCTDecodeParams) -> Result<Vec<u8>> {
    use jpeg_decoder::Decoder;
    let mut decoder = Decoder::new(data);
    let pixels = decoder.decode()?;
    Ok(pixels)
}

fn lzw_decode(mut data: &[u8], params: &LZWFlateParams) -> Result<Vec<u8>> {
    use lzw::{MsbReader, Decoder, DecoderEarlyChange};
    let mut out = vec![];
    if params.early_change != 0 {
        let mut decoder = DecoderEarlyChange::new(MsbReader::new(), 9);
        while data.len() > 0 {
            let (len, bytes) = decoder.decode_bytes(data)?;
            out.extend_from_slice(bytes);

            data = &data[len ..];
        }
    } else {
        let mut decoder = Decoder::new(MsbReader::new(), 9);
        while data.len() > 0 {
            let (len, bytes) = decoder.decode_bytes(data)?;
            out.extend_from_slice(bytes);

            data = &data[len ..];
        }
    }

    Ok(out)
}
fn lzw_encode(data: &[u8], params: &LZWFlateParams) -> Result<Vec<u8>> {
    use lzw::MsbWriter;
    if params.early_change != 0 {
        bail!("encoding early_change != 0 is not supported");
    }
    let mut compressed = vec![];
    lzw::encode(data, MsbWriter::new(&mut compressed), 9).unwrap();
    Ok(compressed)
}

pub fn decode(data: &[u8], filter: &StreamFilter) -> Result<Vec<u8>> {
    match *filter {
        StreamFilter::ASCIIHexDecode => decode_hex(data),
        StreamFilter::ASCII85Decode => decode_85(data),
        StreamFilter::LZWDecode(ref params) => lzw_decode(data, params),
        StreamFilter::FlateDecode(ref params) => flate_decode(data, params),
        StreamFilter::DCTDecode(ref params) => dct_decode(data, params),
        _ => unimplemented!(),
    }
}

pub fn encode(data: &[u8], filter: &StreamFilter) -> Result<Vec<u8>> {
    match *filter {
        StreamFilter::ASCIIHexDecode => Ok(encode_hex(data)),
        StreamFilter::ASCII85Decode => Ok(encode_85(data)),
        StreamFilter::LZWDecode(ref params) => lzw_encode(data, params),
        StreamFilter::FlateDecode (ref params) => Ok(flate_encode(data)),
        _ => unimplemented!(),
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
            0 => Ok(PredictorType::NoFilter),
            1 => Ok(PredictorType::Sub),
            2 => Ok(PredictorType::Up),
            3 => Ok(PredictorType::Avg),
            4 => Ok(PredictorType::Paeth),
            n => Err(PdfError::IncorrectPredictorType {n})
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
            #[allow(clippy::manual_memcpy)]
            // TODO consider: `out[..len].clone_from_slice(&inp[..len])`
            for i in 0..len {
                out[i] = inp[i];
            }
        }
        Sub => {
            for i in bpp..len {
                out[i] = inp[i].wrapping_add(out[i - bpp]);
            }
        }
        Up => {
            for i in 0..len {
                out[i] = inp[i].wrapping_add(prev[i]);
            }
        }
        Avg => {
            for i in 0..bpp {
                out[i] = inp[i].wrapping_add(prev[i] / 2);
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

#[allow(unused)]
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
                current[i] = current[i].wrapping_sub(current[i - bpp].wrapping_add(previous[i]) / 2);
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
