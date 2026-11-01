# enc.rs — stream filters, against PDF 32000-1:2008 §7.4

Reviewed: 2026-11-01.

## Mismatches / gaps

- **`Crypt` filter unimplemented** — `decode()` (~line 527) has no `StreamFilter::Crypt` arm and
  falls through to `bail!("unimplemented")`. 7.4.6 / 7.6.5: a stream may select a crypt filter
  per stream with `/Filter [/Crypt] /DecodeParms << /Name /Foo >>`; that stream fails to decode.
- **CCITTFaxDecode: only `K < 0` (G4) works** (`fax_decode`). `K >= 0` (modified Huffman 1-D and
  G3 2-D, 7.4.6) hits `unimplemented!()` → panic. `BlackIs1`, `EndOfLine`, `EndOfBlock`,
  `EncodedByteAlign`, `DamagedRowsBeforeError` are parsed but only `Columns`/`Rows` are honoured;
  output is always 0/255 grey bytes regardless of `BlackIs1`.
- **ASCIIHexDecode silently drops an odd final digit** (`decode_hex`, `tuples()`). 7.4.2.1: "if
  the filter encounters the end of data with an odd number of digits, the last digit shall be
  assumed to be followed by 0". E.g. `>f>` should decode to 0xF0.
- **TIFF predictor 2 not implemented** (`flate_decode` only handles `predictor > 10`, i.e. the
  PNG predictors 10-15; 7.4.4.4 also defines 2 for TIFF, which is left as raw bytes). A
  `predictor` value of, say, 5 or 20 is also accepted as "PNG".
- **Predictor ignores `BitsPerComponent`**: row stride is `Columns * Colors`, but 7.4.4.4 says
  `ceil(Columns * Colors * BitsPerComponent / 8)`. Any stream with `/BitsPerComponent 1/2/4`
  (common for fax-like images and some object streams) decodes to garbage.
- **Encoding is mostly `unimplemented!()`** (`encode()`): DCT, CCITT, JPX, JBIG2, RunLength and
  Crypt panic when used to write. Only ASCIIHex, ASCII85, LZW, Flate exist (7.4).
- **JPX/JBIG2 need an external decoder** (`set_jpx_decoder`/`set_jbig2_decoder`); unset →
  `"decoder not set"` error, so 7.4.7/7.4.8 images are unreadable in a default build.
- **`JBIG2Globals`** streams are prepended to the page stream with a synthetic file header; the
  spec leaves the embedding to the implementation, but the `.get(jbig2)` segment layout here is
  a heuristic — documents with a real `/JBIG2Globals` header may mis-decode.

## Deliberate deviations (tolerant)

- `flate_decode` accepts raw deflate when zlib framing fails (`inflate_bytes` fallback). 7.4.4
  specifies zlib; accepting both is more forgiving, not less.
- DCTDecode returns rasterised RGB samples (via `zune_jpeg`) rather than the compressed DCT data
  of 7.4.8; `/ColorTransform` is parsed but ignored, and `/Decode`/`/ColorSpace` are applied by
  the caller (`xobject.rs`), not here.
- Decoded output is capped at `object_size_limit` (`read_limited`, `SizeLimitedWriter`) — a
  deliberate hardening not in the spec.

## Checked, no mismatch

- ASCII85: `z` shorthand, partial final group (n chars → n-1 bytes), `~>` terminator with
  nothing after it, whitespace skipping (7.4.3).
- LZW: 9-12 bit codes, `EarlyChange 0/1` mapped onto weezl's TIFF size switch (7.4.4.2).
- RunLength: length 0-127 literal, 129-255 repeat, `EOD = 128` (7.4.5).
- `/Filter` may be a name or an array, `/DecodeParms` may be null or an array with null holes
  (7.4.2) — handled in `object/stream.rs::StreamInfo::from_primitive`, including `/FFilter`
  and `/FDecodeParms`.
