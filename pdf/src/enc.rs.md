# enc.rs — stream filters, against PDF 32000-1:2008 §7.4

Reviewed: 2026-11-01.

Status: the first five items below are **fixed** in this tree (commit "spec: fix wave 2"),
each with a case in `pdf/tests/spec_cases.rs` that failed before the fix: `c08_*` (ASCIIHex
odd digit), `c09_*` (TIFF predictor 2), `c10_*` (row stride vs `BitsPerComponent`),
`c11_*` (Crypt /Identity), and the `StreamFilter::Crypt` variant now carries its `/Name`.
**Correction to the first version of this note:** the "panic" claims below were wrong —
this crate's `unimplemented!()` is a local macro (error.rs:365) that returns an error, so
CCITT `K >= 0` and unsupported `encode()` paths already fail with `Err`. `c12_*` and `c13_*`
are regression guards, not evidence of a bug.

## Mismatches / gaps

- **`Crypt` filter was unimplemented** — `decode()` had no `StreamFilter::Crypt` arm and fell
  through to `bail!("unimplemented")`. 7.4.6 / 7.6.5: a stream may select a crypt filter per
  stream with `/Filter [/Crypt] /DecodeParms << /Name /Foo >>`. `/Identity` (the default) passes
  data through here. Any other name is resolved by the security handler, not by this module:
  `Storage::decode` (`file.rs`) looks the name up in `/CF` and calls `Decoder::decrypt_named`
  before `decode()` ever sees the filter (commit "crypt: honour /StrF, /StmF and per-stream crypt
  filter /Name", cases `c23`/`c24`). This fallback still reports a named filter as an error,
  which is what happens for `Stream` data generated in memory (never encrypted).
- **CCITTFaxDecode: only `K < 0` (G4) works** (`fax_decode`). `K >= 0` (modified Huffman 1-D and
  G3 2-D, 7.4.6) returns "not implemented". `BlackIs1`, `EndOfLine`, `EndOfBlock`,
  `EncodedByteAlign`, `DamagedRowsBeforeError` are parsed but only `Columns`/`Rows` are honoured;
  output is always 0/255 grey bytes regardless of `BlackIs1`. **Not fixed** (needs a 1-D/G3
  decoder); the error path is now the documented behaviour.
- **ASCIIHexDecode dropped an odd final digit** (`decode_hex`). 7.4.2.1: "the last digit shall
  be assumed to be followed by 0", so `f>` is 0xF0, not nothing. Fixed. Note this can only ever
  change the *last byte* of a decoded stream, so it cannot be shown with a rendered page — the
  case is unit-level, checked against the clause.
- **TIFF predictor 2 was not implemented** (`flate_decode` only handled `predictor > 10`).
  7.4.4.4 also defines 2; fixed for `BitsPerComponent` 1, 2, 4, 8 and 16 (row
  padding included), other widths report an error.
  Unknown predictor values now error instead of being silently treated as PNG.
- **Predictor ignored `BitsPerComponent`**: the row stride was `Columns * Colors`, but 7.4.4.4
  says `ceil(Columns * Colors * BitsPerComponent / 8)`; the PNG left-pixel offset is
  `ceil(Colors * BitsPerComponent / 8)` bytes. Fixed. Any 1/2/4-bit image or object stream with
  a predictor decoded to garbage before.
- **Encoding is mostly unimplemented** (`encode()`): DCT, CCITT, JPX, JBIG2, RunLength and Crypt
  return an error (the local macro, so no panic). Only ASCIIHex, ASCII85, LZW, Flate exist (7.4).
- **JPX/JBIG2 need an external decoder** (`set_jpx_decoder`/`set_jbig2_decoder`); unset →
  `"decoder not set"` error, so 7.4.7/7.4.8 images are unreadable in a default build.
- **`JBIG2Globals`** streams are prepended to the page stream with a synthetic file header; the
  spec leaves the embedding to the implementation, but the segment layout here is a heuristic —
  documents with a real `/JBIG2Globals` header may mis-decode.

## BrotliDecode (not in PDF 32000)

Neither ISO 32000-1 nor -2 defines a `/BrotliDecode` filter; it is a de-facto
extension (some compressors write it, pdf.js and others read it). It is
accepted here with the same `/DecodeParms` as Flate — `Predictor`, `Columns`,
`Colors`, `BitsPerComponent` — so PNG and TIFF prediction behave identically,
and `to_primitive` writes it back as `/BrotliDecode`. Decoding is
`brotli-decompressor`, which takes raw and large-window streams and needs no
external dictionary, and the output is capped at `object_size_limit` like every
other filter (`read_limited`, so a decompression bomb is an error, not an OOM).
Image XObjects with this filter are handled by `xobject.rs` alongside Flate.
`encode()` does not implement it: a writer that wants Brotli has to compress
the data itself and set the filter.

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
- ASCIIHex whitespace skipping and the `>` terminator (7.4.2.1); `CCITTFaxDecode`/`Crypt` now
  reachable from `decode()` as well as from the image path (`xobject.rs`).
