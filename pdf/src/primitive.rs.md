# primitive.rs — basic object types, against PDF 32000-1:2008 §7.3, §7.9

Reviewed: 2026-11-01.

## Mismatches

- **Date serialisation omits the closing apostrophe** (`ParsedDate::to_date`, ~line 931):
  `format!("D:{year:04}…{o}{tz_hour:02}'{tz_minute:02}")` yields `D:20261101234500+01'00`.
  7.9.4 Table 23 gives the form `D:YYYYMMDDHHmmSSOHH'mm'` — both apostrophes are required. The
  written date is one character short, and readers that parse strictly reject it.
- `to_date` checks `day > 99` but not month 1-12 or day 1-31 (7.9.4: the field ranges are the
  calendar's), so `D:20261399…` is written happily.
- `to_date` always emits seconds and a time zone. 7.9.4 makes both optional and permits dropping
  trailing fields; writing them is correct, but a `TimeRel::Universal` date is serialised as
  `Z00'00` — legal ("shall be ignored") yet verbose; re-parsing round-trips fine.
- `Date::try_parse` requires the `D:` prefix and a 4-digit year (correct per 7.9.4) but then
  defaults every malformed or missing field to 1/1/0/0/0 rather than reporting the string as
  invalid — tolerant deviation.

## Checked, no mismatch

- `Primitive` variants Null/Boolean/Integer/Number/String/Name/Array/Dictionary/Reference/Stream
  (7.3.2 Table 2) and the `to_primitive` round-trip for each.
- `PdfString`: literal vs hex data preserved (`is_hex`), raw bytes never re-encoded on read.
- Names: `#XX` decoding is *not* applied here — see `parser/mod.rs.md` (keys) and
  `parser/lexer/mod.rs.md` (values are decoded in the parser).
- `/ID` uses `PdfString::data` as raw bytes for key derivation (7.5.5, 7.6.3.3) — no encoding
  assumption.
