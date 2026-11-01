# crypt.rs — against PDF 32000-1:2008 §7.6

Reviewed: 2026-11-01.

## Mismatches

- **`/StrF` is never read.** `CryptDict` (Table 21) parses `/V /R /Length /O /U /OE /UE /P
  /EncryptMetadata /CF` and `/StmF` (as `default_crypt_filter`); `/StrF` has no field and the
  `Decoder` stores a single `method`, so strings are decrypted with the *stream* filter. 7.6.5:
  `/StrF` "shall be used when decrypting strings"; `/StrF /Identity` (strings unencrypted,
  streams RC4/AES) therefore yields garbled strings — and the reverse for `/StmF /Identity`.
- **Per-stream crypt filter `/Name` ignored.** 7.6.5 lets a stream pick any filter from `/CF`
  with `/Filter [/Crypt] /DecodeParms << /Name /Foo >>`; the crate offers only one global
  filter and its `Crypt` stream filter is unimplemented (`enc.rs`). Files with a second crypt
  filter (e.g. embedded-file streams with `/Identity`) fail or are decrypted wrongly.
- **xref streams are decrypted** (`file.rs::Storage::decode`). 7.5.8.2 forbids encrypting them.
- `/V 4` handling accepts only `/StmF`-named filters and errors on unknown `/CFM`
  (`CryptMethod::None` inside a named filter is not treated as Identity). 7.6.5 Table 25:
  `/Identity` is a legal `/StmF`/`/StrF` value meaning "no encryption for this class"; passing
  `/StmF /Identity` with `/V 4` errors out instead of reading the file.
- `Permissions` (`/P`) is decoded but never enforced — conforming to 7.6.4 is optional for
  readers, so this is a stated non-goal rather than a bug.

## Checked, no mismatch

- 7.6.3.3 Algorithm 2 (RC4 40-128 bit key from `/O`, `/P`, `/ID[0]`), Algorithm 3 (`/U` check
  for R2-R4), 7.6.3.4 R5/R6 (SHA-256/384/512 hardened hash loop, `/OE`/`/UE`), AES-128 (`AESV2`)
  and AES-256 (`AESV3`) with a 16-byte IV prefix and PKCS#7 padding.
- Strings inside the `/Encrypt` dictionary are not decrypted (`encrypt_indirect_object`), and
  `/Metadata` is skipped when `/EncryptMetadata false` (7.6.1, 7.5.5).
- Key length is capped per revision (`/Length` 40-128 for R2-R4, 256 for R5/R6) — hardened
  against the unbounded `/Length` crash fixed in the earlier audit.
- Empty strings are returned unchanged in all methods, as required.
