# file.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01.

Status (commit "file: validate the header version, require %%EOF and cap open()",
2026-11-02): the crypt, header and `%%EOF` items are closed, and the "xref streams
are decrypted" claim was **wrong** — see below. Nothing in this file is open any
more; what remains are deliberate choices, listed at the end.

## Mismatches

- ~~**Reachable panics on malformed files**~~ — **fixed**. `scan()` no longer
  `unwrap`s the missing `startxref`/read (the error is yielded as the first item
  of the iterator), and `XRef::Promised` in object loading returns an error
  instead of `unimplemented!()`. 7.5.5 / Annex C expect recovery, not an abort.
- ~~**The header is not validated**~~ — **fixed**. `Storage::version()` (7.5.1)
  now insists on `%PDF-n.m`: four literal characters, a digit, `.`, a digit, and
  returns an error otherwise instead of a slice of whatever follows `%PDF-`.
  `Storage::with_cache` calls it when `allow_xref_error` is cleared, so
  `FileOptions::uncached()` (strict by default) rejects a damaged header while a
  tolerant reader still opens the file. Cases `c30` in `spec_cases.rs`.
- ~~**`%%EOF` is never required or checked**~~ — **fixed**. 7.5.5: "The last line
  of the file shall contain only the end-of-file marker, %%EOF."
  `Storage::check_eof_marker` looks at the last kB and requires the marker with
  only whitespace after it; run when `allow_xref_error` is cleared. A truncated
  file and a file with junk after the marker are both errors in strict mode and
  still readable in tolerant mode. Cases `c31`, `c31b`.
- ~~**Whole-file `std::fs::read` into memory**~~ — **bounded**. `FileOptions` has
  `max_file_size` (default `DEFAULT_MAX_FILE_SIZE`, 1 GiB, `None` = unlimited,
  builder `max_file_size(Option<u64>)`) and `open()` checks the metadata length
  before reading. `load()` with a caller-provided backend is never limited — a
  caller that maps or streams the file is responsible for its own ceiling.
  Case `c32`. Not a spec clause; same class as the `/Length` fixes.
- Trailer `/ID`: only `ID[0]` is used (needed for the key); `ID[1]` is never
  compared. Checked against 7.5.5 Table 15: only informational, and the two
  entries legitimately differ after an incremental update, so there is nothing
  to enforce. No change.
- ~~**One crypt decoder for strings and streams**~~ — fixed. `Storage::decode` now calls
  `Decoder::decrypt_stream` (`/StmF`), the parser's string path uses `Decoder::decrypt`
  (`/StrF`), and a `/Crypt` filter in the stream's own filter array skips the default
  decryption and calls `Decoder::decrypt_named`. Cases `c21`–`c25` in `spec_cases.rs`.
- ~~**xref streams are decrypted**~~ — **wrong**. `load_storage_and_trailer_password` reads the
  xref table/stream and the trailer *before* it constructs the `Decoder`, so `Storage::decode`
  runs with `decoder == None` for them; 7.5.8.2 is satisfied. What *is* real: the `/Encrypt`
  dictionary cannot be used to decrypt the xref stream even if a producer encrypted it, which is
  the spec's intent anyway.

## Checked, no mismatch

- `startxref` offset handling, `trailer` dictionary, `/Prev` chain walking, `/Encrypt` + `/ID`
  key derivation, `/Root` → `/Metadata` lookup for the `EncryptMetadata` exemption, object
  loading through `Ref`/`Promised` for objects written but not yet flushed, `save_to` emitting
  `startxref` and `%%EOF` for both table and stream xref output.
- The version *number* is not range-checked (1.0…1.7, 2.0): the form is what 7.5.1
  prescribes, and a future `%PDF-2.1` should not be refused by a reader that can
  still parse it.
