#![allow(clippy::many_single_char_names)]
#![allow(dead_code)]  // TODO

use crate as pdf;
use crate::error::*;
use crate::object::{Object, ObjectWrite, ParseOptions, Resolve, Stream, Updater};
use crate::primitive::{Primitive, Dictionary, Name};
use std::convert::{TryFrom, TryInto};
use std::io::{Read, Write};
use once_cell::sync::OnceCell;
use datasize::DataSize;


#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
pub struct LZWFlateParams {
    #[pdf(key="Predictor", default="1")]
    pub predictor: i32,
    #[pdf(key="Colors", default="1")]
    pub n_components: i32,
    #[pdf(key="BitsPerComponent", default="8")]
    pub bits_per_component: i32,
    #[pdf(key="Columns", default="1")]
    pub columns: i32,
    #[pdf(key="EarlyChange", default="1")]
    pub early_change: i32,
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

#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
pub struct DCTDecodeParams {
    // TODO The default value of ColorTransform is 1 if the image has three components and 0 otherwise.
    // 0:   No transformation.
    // 1:   If the image has three color components, transform RGB values to YUV before encoding and from YUV to RGB after decoding.
    //      If the image has four components, transform CMYK values to YUVK before encoding and from YUVK to CMYK after decoding.
    //      This option is ignored if the image has one or two color components.
    #[pdf(key="ColorTransform")]
    pub color_transform: Option<i32>,
}

#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
pub struct CCITTFaxDecodeParams {
    #[pdf(key="K", default="0")]
    pub k: i32,

    #[pdf(key="EndOfLine", default="false")]
    pub end_of_line: bool,

    #[pdf(key="EncodedByteAlign", default="false")]
    pub encoded_byte_align: bool,

    #[pdf(key="Columns", default="1728")]
    pub columns: u32,

    #[pdf(key="Rows", default="0")]
    pub rows: u32,

    #[pdf(key="EndOfBlock", default="true")]
    pub end_of_block: bool,

    #[pdf(key="BlackIs1", default="false")]
    pub black_is_1: bool,

    #[pdf(key="DamagedRowsBeforeError", default="0")]
    pub damaged_rows_before_error: u32,
}

#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone)]
pub struct JBIG2DecodeParams {
    #[pdf(key="JBIG2Globals")]
    pub globals: Option<Stream<()>>
}

/// Parameters of the `Crypt` filter (PDF 32000-1:2008 7.4.6, Table 20):
/// `/Name` selects a crypt filter in the document's `/CF` dictionary, the
/// default being `/Identity`.
#[derive(Object, ObjectWrite, Debug, Clone, DataSize, DeepClone, Default)]
pub struct CryptFilterParams {
    #[pdf(key="Name")]
    pub name: Option<Name>,
}

#[derive(Debug, Clone, DataSize, DeepClone)]
pub enum StreamFilter {
    ASCIIHexDecode,
    ASCII85Decode,
    LZWDecode (LZWFlateParams),
    FlateDecode (LZWFlateParams),
    JPXDecode, //Jpeg2k
    DCTDecode (DCTDecodeParams),
    CCITTFaxDecode (CCITTFaxDecodeParams),
    JBIG2Decode(JBIG2DecodeParams),
    Crypt(CryptFilterParams),
    RunLengthDecode
}
impl StreamFilter {
    /// The filter's name, as written in `/Filter` (7.4.1, Table 6). Inline
    /// images use the abbreviations of Table 94, which are handled by the
    /// parser separately.
    pub fn name(&self) -> &'static str {
        match self {
            StreamFilter::ASCIIHexDecode => "ASCIIHexDecode",
            StreamFilter::ASCII85Decode => "ASCII85Decode",
            StreamFilter::LZWDecode(ref _p) => "LZWDecode",
            StreamFilter::FlateDecode(ref _p) => "FlateDecode",
            StreamFilter::JPXDecode => "JPXDecode",
            StreamFilter::DCTDecode(ref _p) => "DCTDecode",
            StreamFilter::CCITTFaxDecode(ref _p) => "CCITTFaxDecode",
            StreamFilter::JBIG2Decode(ref _p) => "JBIG2Decode",
            StreamFilter::Crypt(_) => "Crypt",
            StreamFilter::RunLengthDecode => "RunLengthDecode",
        }
    }

    /// The filter's entry in `/DecodeParms` (7.4.1, Table 5), or `None` for the
    /// filters that take no parameters. A filter with parameters in a chain of
    /// more than one is written as a `null` where it has none, because the
    /// array runs parallel to `/Filter`.
    pub fn params_to_primitive(&self, update: &mut impl Updater) -> Result<Option<Primitive>> {
        Ok(match self {
            StreamFilter::LZWDecode(ref p) => Some(p.to_primitive(update)?),
            StreamFilter::FlateDecode(ref p) => Some(p.to_primitive(update)?),
            StreamFilter::DCTDecode(ref p) => Some(p.to_primitive(update)?),
            StreamFilter::CCITTFaxDecode(ref p) => Some(p.to_primitive(update)?),
            StreamFilter::JBIG2Decode(ref p) => Some(p.to_primitive(update)?),
            StreamFilter::Crypt(ref p) => Some(p.to_primitive(update)?),
            StreamFilter::ASCIIHexDecode
            | StreamFilter::ASCII85Decode
            | StreamFilter::JPXDecode
            | StreamFilter::RunLengthDecode => None,
        })
    }

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
           "CCITTFaxDecode" => StreamFilter::CCITTFaxDecode (CCITTFaxDecodeParams::from_primitive(params, r)?),
           "JBIG2Decode" => StreamFilter::JBIG2Decode(JBIG2DecodeParams::from_primitive(params, r)?),
           "Crypt" => StreamFilter::Crypt(CryptFilterParams::from_primitive(params, r)?),
           "RunLengthDecode" => StreamFilter::RunLengthDecode,
           ty => bail!("Unrecognized filter type {:?}", ty),
       }
       )
    }
}

#[inline]
pub fn decode_nibble(c: u8) -> Option<u8> {
    match c {
        n @ b'0' ..= b'9' => Some(n - b'0'),
        a @ b'a' ..= b'f' => Some(a - b'a' + 0xa),
        a @ b'A' ..= b'F' => Some(a - b'A' + 0xA),
        _ => None
    }
}

#[inline]
fn encode_nibble(c: u8) -> u8 {
    match c {
        0 ..= 9 => b'0'+ c,
        10 ..= 15 => b'a' - 10 + c,
        _ => unreachable!()
    }
}


pub fn decode_hex(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 2);
    let mut high: Option<u8> = None;
    for (i, b) in data.iter().cloned()
        .take_while(|&b| b != b'>')
        .filter(|&b| !matches!(b, 0 | 9 | 10 | 12 | 13 | 32))
        .enumerate()
    {
        let nibble = decode_nibble(b).ok_or(PdfError::HexDecode { pos: i, bytes: [b, 0] })?;
        match high.take() {
            Some(h) => out.push(h << 4 | nibble),
            None => high = Some(nibble),
        }
    }
    // 7.4.2.1: when the filter meets the end of data with an odd number of
    // digits, the last digit shall be assumed to be followed by 0.
    if let Some(h) = high {
        out.push(h << 4);
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
    fn s(b: u8) -> Option<u64> { sym_85(b).map(|n| n as u64) }
    let (a, b, c, d, e) = (s(a)?, s(b)?, s(c)?, s(d)?, s(e)?);
    let q = (((a * 85 + b) * 85 + c) * 85 + d) * 85 + e;
    // 85^5 > 256^4, the result might not fit in an u32.
    let r = u32::try_from(q).ok()?;
    Some(r.to_be_bytes())
}

pub fn decode_85(data: &[u8]) -> Result<Vec<u8>> {
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

fn read_limited(mut reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut data = vec![];
    reader.by_ref().take(limit as u64).read_to_end(&mut data)?;
    if reader.bytes().next().is_some() {
        return Err(PdfError::LimitsExeeded { requested: data.len() + 1 });
    }
    Ok(data)
}

fn inflate_bytes_zlib(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    use libflate::zlib::Decoder;
    let decoder = Decoder::new(data)?;
    read_limited(decoder, limit)
}

fn inflate_bytes(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    use libflate::deflate::Decoder;
    let decoder = Decoder::new(data);
    read_limited(decoder, limit)
}

pub fn flate_decode(data: &[u8], params: &LZWFlateParams, options: &ParseOptions) -> Result<Vec<u8>> {
    if !(0..1024).contains(&params.n_components) {
        bail!("n_components out of range");
    }
    if !(0..1<<20).contains(&params.columns) {
        bail!("columns too large");
    }
    let predictor: usize = params.predictor.try_into()?;
    let n_components: usize = params.n_components.try_into()?;
    let columns: usize = params.columns.try_into()?;
    let bpc: usize = params.bits_per_component.try_into()?;
    if !(1..=16).contains(&bpc) {
        bail!("BitsPerComponent out of range");
    }
    if columns == 0 || n_components == 0 {
        bail!("predictor needs Columns and Colors greater than zero");
    }
    // 7.4.4.4: a row holds Columns × Colors samples of BitsPerComponent bits.
    let row_bits = columns
        .checked_mul(n_components)
        .and_then(|v| v.checked_mul(bpc))
        .ok_or_else(|| PdfError::Other { msg: "predictor row size overflow".into() })?;
    let stride = (row_bits + 7) / 8;
    // The PNG predictors look back one *pixel*, i.e. ceil(Colors×BitsPerComponent/8) bytes.
    let bpp = ((n_components * bpc) + 7) / 8;


    // First flate decode
    let decoded = {
        if let Ok(data) = inflate_bytes_zlib(data, options.object_size_limit) {
            data
        } else if let Ok(data) = inflate_bytes(data, options.object_size_limit) {
            data
        } else {
            dump_data(data);
            bail!("can't inflate");
        }
    };
    // Then unfilter (PNG or TIFF)
    // For this, take the old out as input, and write output to out

    match predictor {
        // 7.4.4.4: 10-15 are the PNG predictors, each row prefixed by its filter byte.
        10..=15 => {
            let inp = decoded; // input buffer
            let rows = inp.len() / (stride+1);

            // output buffer
            check_limits!(rows * stride, options.object_size_limit);
            let mut out = vec![0; rows * stride];

            // Apply inverse predictor
            let null_vec = vec![0; stride];

            let mut in_off = 0; // offset into input buffer

            let mut out_off = 0; // offset into output buffer
            let mut last_out_off = 0; // last offset to output buffer

            while in_off + stride < inp.len() {
                let predictor = PredictorType::from_u8(inp[in_off])?;
                in_off += 1; // +1 because the first byte on each row is predictor

                let row_in = &inp[in_off .. in_off + stride];
                let (prev_row, row_out) = if out_off == 0 {
                    (&null_vec[..], &mut out[out_off .. out_off+stride])
                } else {
                    let (prev, curr) = out.split_at_mut(out_off);
                    (&prev[last_out_off ..], &mut curr[.. stride])
                };
                unfilter(predictor, bpp, prev_row, row_in, row_out);

                last_out_off = out_off;

                in_off += stride;
                out_off += stride;
            }
            Ok(out)
        }
        // 7.4.4.4: predictor 2 is the TIFF horizontal differencing predictor.
        2 => {
            let mut buf = decoded;
            check_limits!(buf.len(), options.object_size_limit);
            tiff_unpredict(&mut buf, stride, n_components, bpc)?;
            Ok(buf)
        }
        1 => Ok(decoded),
        n => bail!("unsupported Predictor {n}"),
    }
}

/// Inverse of the TIFF predictor 2 (horizontal differencing, 7.4.4.4): every
/// sample is stored as the difference to the sample `Colors` positions before
/// it in the same row.
fn tiff_unpredict(data: &mut [u8], stride: usize, n_components: usize, bpc: usize) -> Result<()> {
    if n_components == 0 || stride < n_components {
        bail!("predictor needs Columns and Colors greater than zero");
    }
    match bpc {
        8 => {
            for row in data.chunks_mut(stride) {
                for i in n_components..row.len() {
                    row[i] = row[i].wrapping_add(row[i - n_components]);
                }
            }
        }
        16 => {
            let comps = n_components;
            for row in data.chunks_mut(stride) {
                let samples = row.len() / 2;
                for i in comps..samples {
                    let prev_at = 2 * (i - comps);
                    let at = 2 * i;
                    let prev = u16::from_be_bytes([row[prev_at], row[prev_at + 1]]);
                    let cur = u16::from_be_bytes([row[at], row[at + 1]]);
                    row[at .. at + 2].copy_from_slice(&cur.wrapping_add(prev).to_be_bytes());
                }
            }
        }
        _ => bail!("TIFF Predictor with BitsPerComponent {} is not implemented", bpc),
    }
    Ok(())
}
fn flate_encode(data: &[u8], params: &LZWFlateParams) -> Vec<u8> {
    if params.predictor == 1 {
        use libflate::zlib::Encoder;
        let mut encoder = Encoder::new(vec![]).unwrap();
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap().0
    } else {
        use libflate::deflate::Encoder;
        let mut encoder = Encoder::new(Vec::new());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap().0
    }
}

pub fn dct_decode(data: &[u8], _params: &DCTDecodeParams, options: &ParseOptions) -> Result<Vec<u8>> {
    use zune_jpeg::{
        JpegDecoder, zune_core::{
            options::DecoderOptions as ZuneOptions,
            bytestream::ZCursor,
        }
    };
    let zune_options;

    if options.safe_jpeg {
        zune_options = ZuneOptions::new_safe();
    } else {
        zune_options = ZuneOptions::new_fast();
    }
    let zune_options = zune_options.set_max_height(options.max_img_dim).set_max_width(options.max_img_dim);

    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), zune_options);
    decoder.decode_headers()?;
    let info = try_opt!(decoder.info(), "no JPEG headers");
    let requested = info.height as usize * info.width as usize * info.components as usize;
    check_limits!(requested, options.object_size_limit);

    let pixels = decoder.decode()?;
    Ok(pixels)
}

/// Writes at most `limit` bytes into a `Vec`, failing the decode once the
/// output would grow past it. The decoders below are fed by the file, so an
/// output size can not be trusted without a ceiling.
struct SizeLimitedWriter<'a> {
    buf: &'a mut Vec<u8>,
    limit: usize,
}
impl std::io::Write for SizeLimitedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.buf.len() + data.len() > self.limit {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "decoded stream exceeds object_size_limit"));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn lzw_decode(data: &[u8], params: &LZWFlateParams, options: &ParseOptions) -> Result<Vec<u8>> {
    use weezl::{BitOrder, decode::Decoder};
    let mut out = vec![];

    let mut decoder = if params.early_change != 0 {
        Decoder::with_tiff_size_switch(BitOrder::Msb, 9)
    } else {
        Decoder::new(BitOrder::Msb, 9)
    };

    let mut writer = SizeLimitedWriter { buf: &mut out, limit: options.object_size_limit };
    decoder
        .into_stream(&mut writer)
        .decode_all(data).status?;
    check_limits!(out.len(), options.object_size_limit);
    Ok(out)
}
fn lzw_encode(data: &[u8], params: &LZWFlateParams) -> Result<Vec<u8>> {
    use weezl::{BitOrder, encode::Encoder};
    let mut encoder;

    if params.early_change == 0 {
        encoder = Encoder::new(BitOrder::Msb, 9);
    } else {
        encoder = Encoder::with_tiff_size_switch(BitOrder::Msb, 9);
    }
    let compressed = encoder.encode(data).unwrap();
    Ok(compressed)
}

pub fn fax_decode(data: &[u8], params: &CCITTFaxDecodeParams, options: &ParseOptions) -> Result<Vec<u8>> {
    use fax::{Color, decoder::{pels, decode_g4}};

    if params.k < 0 {
        let columns: u16 = params.columns.try_into()?;
        let rows: u16 = params.rows.try_into()?;
        if columns == 0 {
            bail!("CCITTFaxDecode: Columns needs to be greater than zero");
        }

        // `/Rows 0` means "as many as the encoded data holds" (7.4.5), so the
        // height is decided by the decoder and the output cannot be sized from
        // the file. It is capped like every other decoded stream instead; the
        // decode is abandoned as soon as the cap is crossed.
        let stride = columns as usize;
        let rows_requested = if rows == 0 { None } else { Some(rows as usize) };
        let limit = options.object_size_limit;
        let mut buf: Vec<u8> = Vec::with_capacity(rows_requested.map_or(0, |r| stride.saturating_mul(r)).min(limit));
        let mut over = None;
        let height = rows_requested.map(|r| u16::try_from(r)).transpose()?;

        decode_g4(data.iter().cloned(), columns, height, |line| {
            if over.is_some() {
                return;
            }
            buf.extend(pels(line, columns).map(|c| match c {
                Color::Black => 0,
                Color::White => 255
            }));
            if buf.len() > limit {
                over = Some(buf.len());
            }
        }).ok_or(PdfError::Other { msg: "faxdecode failed".into() })?;

        if let Some(requested) = over {
            err!(PdfError::LimitsExeeded { requested });
        }
        if let Some(rows) = rows_requested {
            if buf.len() != stride.saturating_mul(rows) {
                bail!("decoded length does not match (expected {rows}∙{columns}, got {})", buf.len());
            }
        } else if buf.len() % stride != 0 {
            bail!("decoded {} bytes, which is not a whole number of {columns} pixel rows", buf.len());
        }
        Ok(buf)
    } else {
        // 7.4.5 CCITTFaxDecode: K = 0 is modified Huffman (1-D), K > 0 is
        // mixed 1-D/2-D G3. pdf-rs only decodes G4 (K < 0); say so instead of
        // panicking, so a hostile or unusual file is an error, not a crash.
        bail!("CCITTFaxDecode with K = {} is not implemented (only K < 0, group 4)", params.k)
    }
}

pub fn run_length_decode(data: &[u8], options: &ParseOptions) -> Result<Vec<u8>> {
    // Used <http://benno.id.au/refs/PDFReference15_v5.pdf> as specification
    let mut buf = Vec::new();
    let d = data;
    let mut c = 0;

    while c < data.len() {
        let length = d[c]; // length is first byte
        if length < 128 {
            let start = c + 1;
            let end = start + length as usize + 1;
            // copy _following_ length + 1 bytes literally
            let source = d.get(start..end).ok_or(PdfError::RleError)?;
            check_limits!(buf.len() + source.len(), options.object_size_limit);
            buf.extend_from_slice(source);
            c = end; // move cursor to next run
        } else if length >= 129 {
            let copy = 257 - length as usize; // copy 2 - 128 times
            let b = *d.get(c + 1).ok_or(PdfError::RleError)?; // copied byte
            check_limits!(buf.len() + copy, options.object_size_limit);
            buf.extend(std::iter::repeat(b).take(copy));
            c += 2; // move cursor to next run
        } else {
            break; // EOD
        }
    }

    Ok(buf)
}

pub type DecodeFn = dyn Fn(&[u8]) -> Result<Vec<u8>> + Sync + Send + 'static;
static JPX_DECODER: OnceCell<Box<DecodeFn>> = OnceCell::new();
static JBIG2_DECODER: OnceCell<Box<DecodeFn>> = OnceCell::new();

pub fn set_jpx_decoder(f: Box<DecodeFn>) {
    let _ = JPX_DECODER.set(f);
}
pub fn set_jbig2_decoder(f: Box<DecodeFn>) {
    let _ = JBIG2_DECODER.set(f);
}

pub fn jpx_decode(data: &[u8]) -> Result<Vec<u8>> {
    JPX_DECODER.get().ok_or_else(|| PdfError::Other { msg: "jp2k decoder not set".into()})?(data)
}
pub fn jbig2_decode(data: &[u8], globals: &[u8]) -> Result<Vec<u8>> {
    let data = [
        // file header
        // &[0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x00, 0x00, 0x00, 0x01],

        globals,
        data,

        // end of page
        &[0x00, 0x00, 0x00, 0x03, 0x31, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],

        // end of stream
        &[0x00, 0x00, 0x00, 0x04, 0x33, 0x01, 0x00, 0x00, 0x00, 0x00],
    ].concat();
    JBIG2_DECODER.get().ok_or_else(|| PdfError::Other { msg: "jbig2 decoder not set".into()})?(&data)
}

pub fn decode(data: &[u8], filter: &StreamFilter, options: &ParseOptions) -> Result<Vec<u8>> {
    match *filter {
        StreamFilter::ASCIIHexDecode => decode_hex(data),
        StreamFilter::ASCII85Decode => decode_85(data),
        StreamFilter::LZWDecode(ref params) => lzw_decode(data, params, options),
        StreamFilter::FlateDecode(ref params) => flate_decode(data, params, options),
        StreamFilter::RunLengthDecode => run_length_decode(data, options),
        StreamFilter::DCTDecode(ref params) => dct_decode(data, params, options),
        StreamFilter::CCITTFaxDecode(ref params) => fax_decode(data, params, options),
        StreamFilter::Crypt(ref params) => crypt_filter_decode(data, params),

        _ => bail!("unimplemented {filter:?}"),
    }
}

/// PDF 32000-1:2008 7.4.6: with `/Name /Identity` the crypt filter performs no
/// encryption. Any other name selects a filter from the document's `/CF`
/// dictionary, applied by the security handler rather than here.
fn crypt_filter_decode(data: &[u8], params: &CryptFilterParams) -> Result<Vec<u8>> {
    match params.name.as_ref().map_or("Identity", |n| n.as_str()) {
        "Identity" => Ok(data.to_vec()),
        name => bail!("Crypt filter /{} needs the document's security handler", name),
    }
}

pub fn encode(data: &[u8], filter: &StreamFilter) -> Result<Vec<u8>> {
    match *filter {
        StreamFilter::ASCIIHexDecode => Ok(encode_hex(data)),
        StreamFilter::ASCII85Decode => Ok(encode_85(data)),
        StreamFilter::LZWDecode(ref params) => lzw_encode(data, params),
        StreamFilter::FlateDecode (ref params) => Ok(flate_encode(data, params)),
        // Writing DCT/CCITT/JPX/JBIG2/RunLength/Crypt encoded data is not
        // implemented; report it instead of panicking on a user request.
        _ => bail!("encoding with {filter:?} is not implemented"),
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
    if bpp > len {
        return;
    }

    match filter {
        NoFilter => {
            out[..len].copy_from_slice(&inp[..len]);
        }
        Sub => {
            out[..bpp].copy_from_slice(&inp[..bpp]);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_85() {
        fn s(b: &[u8]) -> &str { std::str::from_utf8(b).unwrap() }

        let case = &b"hello world!"[..];
        let encoded = encode_85(case);
        assert_eq!(s(&encoded), "BOu!rD]j7BEbo80~>");
        let decoded = decode_85(&encoded).unwrap();
        assert_eq!(case, &*decoded);
        /*
        assert_eq!(
            s(&decode_85(
                &lzw_decode(
                    &decode_85(&include_bytes!("data/t01_lzw+base85.txt")[..]).unwrap(),
                    &LZWFlateParams::default()
                ).unwrap()
            ).unwrap()),
            include_str!("data/t01_plain.txt")
        );
        */
    }

    #[test]
    fn run_length_decode_test() {
        let x = run_length_decode(&[254, b'a', 255, b'b', 2, b'c', b'b', b'c', 254, b'a', 128], &ParseOptions::strict()).unwrap();
        assert_eq!(b"aaabbcbcaaa", x.as_slice());
    }

    #[test]
    fn caps_are_enforced() {
        let tiny = ParseOptions { object_size_limit: 4, ..ParseOptions::strict() };
        let encoded = lzw_encode(b"aaaaaaaaaaaaaaaa", &LZWFlateParams::default()).unwrap();
        assert!(lzw_decode(&encoded, &LZWFlateParams::default(), &tiny).is_err());
        // two 3 byte repeat runs, 6 bytes in total, over a limit of 4
        assert!(run_length_decode(&[254, b'a', 254, b'b'], &tiny).is_err());
    }
}
