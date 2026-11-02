# crypt.rs — against PDF 32000-1:2008 §7.6

Reviewed: 2026-11-01.

## Status (commit "crypt: honour /StrF, /StmF and per-stream crypt filter /Name")

Cases in `pdf/tests/spec_cases.rs`: `fixtures_crypt_control_both_default` (`c20`),
`c21_strings_identity_streams_encrypted`, `c22_streams_identity_strings_encrypted`,
`c23_stream_crypt_name_identity`, `c24_stream_crypt_name_from_cf`,
`c25_strings_default_to_identity`; fixtures in `/home/hematite/report/pdf-cases/`.
On unpatched master c21–c25 fail; c20 (control: `/StmF` = `/StrF` = a named RC4 filter) passes
on master too, which is what makes it a usable control for the generator.

Fixed here:

- **`/StrF` was never read.** 7.6.5 Table 20: `/StrF` is the default crypt filter for
  *strings*, `/StmF` the one for *streams*, both defaulting to `/Identity`. `CryptDict` now has
  `string_crypt_filter: Option<Name>`, `Decoder` has two independent `FilterSpec`s
  (`string`, `stream`) and `Decoder::decrypt` (the string path, used by `parser/mod.rs`) uses
  `/StrF` while `Decoder::decrypt_stream` uses `/StmF`.
- **Per-stream crypt filter `/Name` was ignored.** 7.6.5: `/Filter [/Crypt]` overrides `/StmF`
  for that stream, `/DecodeParms /Name` selecting any entry of `/CF`; no name means
  `/Identity`. `Decoder` now keeps the `/CF` table (`filters: HashMap<Name, FilterSpec>`),
  `Storage::decode` (`file.rs`) skips the default decryption when the filter chain contains
  `/Crypt` and calls `Decoder::decrypt_named` for it.
- **`/V 4` with `/StmF /Identity` (or `/StrF /Identity`) failed** with "missing crypt filter
  entry". `/Identity` is a legal value (Table 20) and is now a `FilterSpec` with
  `CryptMethod::None`; a named filter with `/CFM None` behaves the same (Table 25).
- Key length: for `/V 4..6` the file key length now comes from whichever of `/StmF`/`/StrF`
  actually encrypts something (falling back to `/Length`); per-filter `/Length` is used
  per object (Algorithm 1 b). `/V 1..2` still applies the one standard RC4 filter to everything,
  since those files have no `/CF` (7.6.1) — without this the `/StrF` default of `/Identity`
  would break every RC4 R2/R3 file, which `tests/integration.rs::decrypt_streams` caught.
- Removed the `unreachable!()` for `CryptMethod::None` in the decryption path (now a legal
  no-op), added `PartialEq`/`Eq` to `CryptMethod`.

## Corrections to the earlier audit

- **"xref streams are decrypted" was wrong.** `Storage::load_storage_and_trailer_password`
  reads the xref table and trailer *before* it builds the `Decoder`, so
  `Storage::decode` runs with `decoder == None` for them; 7.5.8.2 is satisfied. Nothing changed
  here.

## Checked, no mismatch

- 7.6.3.3 Algorithm 2 (RC4 40-128 bit key from `/O`, `/P`, `/ID[0]`), Algorithm 3 (`/U` check
  for R2-R4), 7.6.3.4 R5/R6 (SHA-256/384/512 hardened hash loop, `/OE`/`/UE`), AES-128 (`AESV2`)
  and AES-256 (`AESV3`) with a 16-byte IV prefix and PKCS#7 padding.
- Strings inside the `/Encrypt` dictionary are not decrypted (`encrypt_indirect_object`), and
  `/Metadata` is skipped when `/EncryptMetadata false` (7.6.1, 7.5.5) — for streams through
  `decrypt_with`, and for strings through the object-id check.
- Key length is capped per revision (`/Length` 40-128 for R2-R4, 256 for R5/R6) — hardened
  against the unbounded `/Length` crash fixed in the earlier audit.
- Empty strings are returned unchanged in all methods, as required.
- `Permissions` (`/P`) is decoded but never enforced — conforming to 7.6.4 is optional for
  readers, so this is a stated non-goal rather than a bug.

## Second reader

mupdf (via `pdf_to_markdown`) renders c20–c23 and c25 as "Hello", which confirms the generator
and the string/stream split from the outside. c24 is *rejected* by mupdf with "Crypt Filter not
Identity or StdCF": mupdf only knows the filter name `/StdCF`, while 7.6.5 allows any name from
`/CF`. The pdf-rs behaviour for that case is therefore pinned by the test against the spec text
alone.
