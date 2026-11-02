# file.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01.

Status (commit "crypt: honour /StrF, /StmF and per-stream crypt filter /Name"): the crypt items
are fixed, and the "xref streams are decrypted" claim was **wrong** — see below.
`version()` and the `%%EOF` check are still open.

## Mismatches

- **Reachable panics on malformed files** (7.5.5 / Annex C expect recovery instead):
  - `scan()` line ~238: `self.backend.locate_xref_offset().unwrap()` and the following
    `read(...).unwrap()` — a file without a usable `startxref` aborts the process.
  - `XRef::Promised` in object loading → `unimplemented!()`.
  - `%%EOF` is never required or checked, and `version()` simply slices `backend[start+1 .. start+8]`
    without validating `%PDF-1.n` (7.5.5, 7.5.1). A wrong or missing header changes nothing.
- ~~**One crypt decoder for strings and streams**~~ — fixed. `Storage::decode` now calls
  `Decoder::decrypt_stream` (`/StmF`), the parser's string path uses `Decoder::decrypt`
  (`/StrF`), and a `/Crypt` filter in the stream's own filter array skips the default
  decryption and calls `Decoder::decrypt_named`. Cases `c21`–`c25` in `spec_cases.rs`.
- ~~**xref streams are decrypted**~~ — **wrong**. `load_storage_and_trailer_password` reads the
  xref table/stream and the trailer *before* it constructs the `Decoder`, so `Storage::decode`
  runs with `decoder == None` for them; 7.5.8.2 is satisfied. What *is* real: the `/Encrypt`
  dictionary cannot be used to decrypt the xref stream even if a producer encrypted it, which is
  the spec's intent anyway.
- Whole-file `std::fs::read` into memory (`FileOptions`/backend, ~line 774) — no size ceiling;
  not a spec clause, but the same class of issue as the earlier `/Length` fixes.
- Trailer `/ID`: only `ID[0]` is used (needed for the key); `ID[1]` is never compared, so files
  whose two IDs disagree are not noticed (7.5.5 Table 15 - informational).

## Checked, no mismatch

- `startxref` offset handling, `trailer` dictionary, `/Prev` chain walking, `/Encrypt` + `/ID`
  key derivation, `/Root` → `/Metadata` lookup for the `EncryptMetadata` exemption, object
  loading through `Ref`/`Promised` for objects written but not yet flushed, `save_to` emitting
  `startxref` and `%%EOF` for both table and stream xref output.
