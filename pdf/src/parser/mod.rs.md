# parser/mod.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01.

Status: the first two mismatches below are **fixed** in this tree (commit "spec: fix wave 1");
the name decoding now lives in one helper (`decode_name`) used for keys and values. Cases:
`c05_dict_key_is_hex_decoded`, `fixtures_escaped_font_key`, `c06_null_dict_value_counts_as_absent`.

## Mismatches

- **Dictionary keys are not `#XX`-decoded** (`parse_dictionary_object`, ~line 144:
  `key_token.reslice(1..).to_name()`), while name *values* are decoded correctly at ~line 206.
  7.3.5 applies the `#` escape to every name, keys included: `/A#20B true` yields the key
  `A#20B` and is unreachable as `A B`.
- **Null dictionary values are kept** (~line 144, `dict.insert(key, obj)`). 7.3.9: "a dictionary
  entry whose value is null shall be treated in the same way as an entry that is not present".
  Typed readers therefore see `Some(Null)` where the spec says absent (`/Length null`,
  `/Type null`, `/DecodeParms [null]`).
- **Strings are decrypted with the stream filter** (`Context::decrypt`, line 46 → resolves to
  the single `crypt::Decoder` built from `/StmF`). See `crypt.rs.md`; the string path is here
  because this is where every `PdfString` is decrypted (lines 259-261, 278-280).

## Deliberate deviations (tolerant reader)

- Integers are parsed as `i64` (7.3.3 allows only 32-bit); `4294967292` parses and survives.
  `as_integer()` still rejects out-of-range values, so the deviation is only in the parser.
- Reals are parsed into `f32`; the spec's range fits, but digits are lost past ~7 significant
  digits and the "at most 5 decimal places" rule is not enforced.

## Checked, no mismatch

- Indirect object syntax `n g obj … endobj`, `stream`/`endstream` with `/Length` (including an
  indirect `/Length`), object nesting depth limit, xref-stream object parsing.
