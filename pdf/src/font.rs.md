# font.rs — fonts, against PDF 32000-1:2008 §9.5-9.10

Reviewed: 2026-11-01.

## Mismatches

- **`/MissingWidth` is ignored for simple fonts** (`Font::widths`, line 328): a `TFont` with
  `/FirstChar`/`/Widths` produces `Widths { default: 0.0, .. }`. 9.6.2.1: a code outside
  `FirstChar … LastChar` — and every code when `/Widths` is absent — takes the glyph width from
  the font descriptor's `/MissingWidth`. Text using code 32 without an entry in `/Widths`
  therefore measures 0 instead of `MissingWidth`, shifting every following glyph.
- **`/Widths` length is not checked against `/LastChar`**: 9.6.2.1 requires exactly
  `LastChar - FirstChar + 1` entries; a short array silently truncates (codes past the end get
  width 0), a long one is ignored beyond the end. Neither is reported.
- **`descendant_fonts[0]` panics** (line 327) when a Type 0 font has an empty or missing
  `/DescendantFonts`. 9.7.4 requires exactly one entry, so this is malformed input — but the
  failure mode is an index panic, not an error value.
- Type 0 `/W` parsing: an empty sub-array `[c []]` is handled (fixed earlier), but the `c1 > c2`
  ordering of the `c1 c2 w` form is not validated and produces an empty range instead of an error
  (9.7.4.3 — tolerant deviation).
- **`/Encoding` differences and `/ToUnicode`**: only the font's `/Encoding` object and a
  `ToUnicodeMap` built from `/ToUnicode` are kept; there is no `/Differences`-based glyph-name →
  Unicode mapping, so text extraction depends entirely on `/ToUnicode` (9.10.2). Fonts without it
  (common in older files) fall back to the raw byte code.
- Type 3: `/FontMatrix` scaling by 1/1000 is not applied anywhere in width computation
  (9.6.5) — widths are taken as if the matrix were the default.

## Checked, no mismatch

- `/Subtype` dispatch over Type0, Type1, MMType1, TrueType, CIDFontType0, CIDFontType2, Type3;
  `/BaseFont` required except for Type 3 (9.6.5/9.6.2).
- CID `/W` array forms `c [w1 w2 …]` and `c_first c_last w`, with indirect arrays resolved,
  `/DW` default 1000 (9.7.4.3).
- `ToUnicode` CMap subset (`beginbfchar`, `beginbfrange`, indirect `/ToUnicode` streams) and the
  reverse writer `write_cmap`.
- Font descriptor fields incl. `/MissingWidth` default 0, `/Flags`, `/FontBBox`, `/StemV`.
