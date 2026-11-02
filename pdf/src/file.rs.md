# file.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01.

Status (commit "spec: fix wave 3"): `scan()` no longer unwraps — a missing/unusable `startxref`
now yields a single `Err` item instead of aborting. Correcting the rest of this note: `scan()` is
*not* on the `load()` path (nothing in the crate calls it, and a successful `load()` implies a
usable `startxref`), and `XRef::Promised` hits the crate's error-returning `unimplemented!`
macro, not a panic. `version()`, the `%%EOF` check and the `/StrF` item are still open.

## Mismatches

- **Reachable panics on malformed files** (7.5.5 / Annex C expect recovery instead):
  - `scan()` line ~238: `self.backend.locate_xref_offset().unwrap()` and the following
    `read(...).unwrap()` — a file without a usable `startxref` aborts the process.
  - `XRef::Promised` in object loading → `unimplemented!()`.
  - `%%EOF` is never required or checked, and `version()` simply slices `backend[start+1 .. start+8]`
    without validating `%PDF-1.n` (7.5.5, 7.5.1). A wrong or missing header changes nothing.
- **One crypt decoder for strings and streams** (`Storage::decode`, lines 155-165, and
  `parse_indirect_object(..., self.decoder, ...)`): the decoder is built from `/StmF` only, then
  applied to every string and every stream. 7.6.5 defines `/StrF` separately, and a file with
  `/StrF /Identity` (strings in the clear, streams encrypted) is read wrongly. See `crypt.rs.md`.
- **xref streams are decrypted** (`Storage::decode` decrypts before any filter runs). 7.5.8.2:
  "The cross-reference stream shall not be encrypted". Files that (illegally) encrypt it are read
  correctly by accident; files relying on the exemption with an unreadable `/Encrypt` dictionary
  fail. Also `/Metadata` is exempted only via `EncryptMetadata`, which is the spec'd behaviour.
- Whole-file `std::fs::read` into memory (`FileOptions`/backend, ~line 774) — no size ceiling;
  not a spec clause, but the same class of issue as the earlier `/Length` fixes.
- Trailer `/ID`: only `ID[0]` is used (needed for the key); `ID[1]` is never compared, so files
  whose two IDs disagree are not noticed (7.5.5 Table 15 - informational).

## Checked, no mismatch

- `startxref` offset handling, `trailer` dictionary, `/Prev` chain walking, `/Encrypt` + `/ID`
  key derivation, `/Root` → `/Metadata` lookup for the `EncryptMetadata` exemption, object
  loading through `Ref`/`Promised` for objects written but not yet flushed, `save_to` emitting
  `startxref` and `%%EOF` for both table and stream xref output.
